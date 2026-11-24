module Transcription {
  type Speaker = SpeakerA | SpeakerB
  type Target = { speakers: Set[Speaker], start: int, end: int }
  type Word = { speaker: Speaker, text: str, start: int, end: int }
  type State = {
    target: Target,
    words: Set[Word],
    nameA: str,
    captions: Set[Word],
    pending: Set[Word],
    preserved: Set[Word],
    beforeReplacement: Set[Word],
    warning: bool,
  }

  var state: State

  pure val everyone = Set(SpeakerA, SpeakerB)
  pure val wholeSource = { speakers: everyone, start: 0, end: 120 }
  pure val noTarget = { speakers: Set(), start: 0, end: 0 }
  // ASCII transliterations stand for Japanese words in the local recognizer's output.
  pure val wordA = { speaker: SpeakerA, text: "konnichiwa", start: 32, end: 33 }
  pure val wordB = { speaker: SpeakerB, text: "ryoukai", start: 35, end: 36 }
  pure val wordOutside = { speaker: SpeakerA, text: "sayounara", start: 71, end: 74 }
  pure val sourceWords = Set(wordA, wordB, wordOutside)
  pure val editedA = { ...wordA, text: "edited" }
  pure val captionB = { speaker: SpeakerB, text: "speaker-b-caption", start: 34, end: 36 }
  pure val outside = { speaker: SpeakerA, text: "outside-range", start: 71, end: 74 }
  pure val initialCaptions = Set(editedA, captionB, outside)

  action init = state' = {
    target: noTarget,
    words: Set(),
    nameA: "speaker-a",
    captions: initialCaptions,
    pending: Set(),
    preserved: Set(),
    beforeReplacement: Set(),
    warning: false,
  }

  pure def within(target, word) =
    target.speakers.contains(word.speaker) and target.start <= word.start and word.end <= target.end

  action recognize = all {
    state.target != noTarget,
    state' = { ...state, words: sourceWords.filter(word => within(state.target, word)) },
  }

  action setSpeakerName = state' = { ...state, nameA: "Hanako" }

  val displayCaptions = state.words.map(word => {
    speaker: if (word.speaker == SpeakerA) state.nameA else "speaker-b",
    text: word.text,
    start: word.start,
    end: word.end,
  })

  action requestSource(target) = all {
    not(state.warning),
    target.speakers != Set(),
    0 <= target.start and target.start < target.end and target.end <= 120,
    state' = { ...state, target: target, words: Set() },
  }

  action requestTimeline(selectionStart, selectionEnd) = {
    val timelineStart = 10
    val sourceStart = 30
    val duration = 10
    all {
      timelineStart <= selectionStart,
      selectionEnd <= timelineStart + duration,
      requestSource({
        speakers: everyone,
        start: sourceStart + selectionStart - timelineStart,
        end: sourceStart + selectionEnd - timelineStart,
      }),
    }
  }

  pure def overlaps(target, caption) =
    target.speakers.contains(caption.speaker) and caption.start < target.end and target.start < caption.end

  action proposeReplacement = all {
    not(state.warning),
    state.captions.contains(editedA),
    state.words == Set(wordA),
    state.target.speakers == Set(SpeakerA),
    overlaps(state.target, editedA),
    not(overlaps(state.target, outside)),
    state' = {
      ...state,
      pending: state.words,
      preserved: state.captions.filter(caption => not(overlaps(state.target, caption))),
      beforeReplacement: state.captions,
      warning: true,
    },
  }

  action resolveReplacement(replace) = all {
    state.warning,
    state' = {
      ...state,
      captions: if (replace) state.preserved.union(state.pending) else state.captions,
      pending: Set(),
      warning: false,
    },
  }

  action step = any {
    recognize,
    setSpeakerName,
    requestSource(wholeSource),
    requestSource({ ...wholeSource, speakers: Set(SpeakerA) }),
    requestSource({ ...wholeSource, start: 30, end: 60 }),
    requestSource({ speakers: Set(SpeakerB), start: 30, end: 60 }),
    requestSource({ speakers: Set(SpeakerA), start: 30, end: 60 }),
    requestTimeline(12, 14),
    proposeReplacement,
    resolveReplacement(false),
    resolveReplacement(true),
  }

  val workflowRules = all {
    state.target.speakers.subseteq(everyone),
    (state.target != noTarget) implies
      (0 <= state.target.start and state.target.start < state.target.end and state.target.end <= 120),
    state.words.forall(word =>
      within(state.target, word) and word.text != "" and
        0 <= word.start and word.start < word.end and word.end <= 120),
    state.warning == (state.pending != Set()),
    state.preserved.subseteq(state.captions),
    state.warning implies all {
      state.captions == state.beforeReplacement,
      state.captions.contains(editedA),
      state.pending.forall(word => overlaps(state.target, word)),
      state.captions.filter(caption => not(overlaps(state.target, caption))) == state.preserved,
    },
  }

  // BDD: transcription.feature:4
  run localJapaneseSourceTimesAndSpeakersTest = init.then(requestSource(wholeSource)).then(recognize)
    .expect(state.words == Set(wordA, wordB, wordOutside))
    .then(setSpeakerName)
    .expect(displayCaptions.contains({ speaker: "Hanako", text: "konnichiwa", start: 32, end: 33 })
      and displayCaptions.contains({ speaker: "speaker-b", text: "ryoukai", start: 35, end: 36 }))

  // BDD: transcription.feature:12 [19]
  run allAudioTest = init.then(requestSource(wholeSource))
    .expect(state.target == { speakers: Set(SpeakerA, SpeakerB), start: 0, end: 120 })
    .then(recognize).expect(state.words == Set(wordA, wordB, wordOutside))

  // BDD: transcription.feature:12 [20]
  run wholeSpeakerATest = init.then(requestSource({ ...wholeSource, speakers: Set(SpeakerA) }))
    .expect(state.target == { speakers: Set(SpeakerA), start: 0, end: 120 })
    .then(recognize).expect(state.words == Set(wordA, wordOutside))

  // BDD: transcription.feature:12 [21]
  run sourceIntervalTest = init.then(requestSource({ ...wholeSource, start: 30, end: 60 }))
    .expect(state.target == { speakers: Set(SpeakerA, SpeakerB), start: 30, end: 60 })
    .then(recognize).expect(state.words == Set(wordA, wordB))

  // BDD: transcription.feature:12 [22]
  run repeatSpeakerBIntervalTest = init.then(requestSource(wholeSource)).then(recognize)
    .then(requestSource({ speakers: Set(SpeakerB), start: 30, end: 60 }))
    .expect(state.target == { speakers: Set(SpeakerB), start: 30, end: 60 })
    .then(recognize).expect(state.words == Set(wordB))

  // BDD: transcription.feature:24
  run timelineIntervalToSourceTest = init.then(requestTimeline(12, 14))
    .expect(state.target == { speakers: Set(SpeakerA, SpeakerB), start: 32, end: 34 })
    .then(recognize).expect(state.words == Set(wordA))

  // BDD: transcription.feature:29 [41]
  run acceptScopedReplacementTest = init
    .then(requestSource({ speakers: Set(SpeakerA), start: 30, end: 60 }))
    .then(recognize).then(proposeReplacement)
    .expect(state.warning and state.captions == initialCaptions)
    .then(resolveReplacement(true))
    .expect(not(state.warning) and state.captions == Set(wordA, captionB, outside))

  // BDD: transcription.feature:29 [42]
  run retainEditedCaptionsTest = init
    .then(requestSource({ speakers: Set(SpeakerA), start: 30, end: 60 }))
    .then(recognize).then(proposeReplacement)
    .expect(state.warning and state.captions == initialCaptions)
    .then(resolveReplacement(false))
    .expect(not(state.warning) and state.captions == initialCaptions)
}
