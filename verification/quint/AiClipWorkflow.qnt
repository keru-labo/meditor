module AiClipWorkflow {
  type Range = { start: int, end: int }
  type State = {
    clip: Range,
    savedClip: Range,
    savedSource: int,
    staged: Range,
    source: int,
    transcript: str,
    request: str,
    status: str,
    beforeClip: Range,
    beforeRequest: str,
    last: str,
  }

  var state: State

  pure def interval(start, end) = { start: start, end: end }
  pure val noClip = interval(-1, -1)
  pure val selected = interval(30, 40)
  pure val manual = interval(31, 39)
  pure val firstRemark = interval(33, 36)
  pure val secondRemark = interval(62, 65)
  pure def valid(r) = 0 <= r.start and r.start < r.end and r.end <= 90
  pure def includes(outer, inner) = outer.start <= inner.start and inner.end <= outer.end
  pure def disjoint(a, b) = a.end <= b.start or b.end <= a.start
  pure def remarks(transcript) =
    if (transcript == "missing") Set()
    else if (transcript == "unrelated") Set(secondRemark)
    else Set(firstRemark, secondRemark)
  pure def hasRelevantRemark(transcript) = remarks(transcript).contains(firstRemark)
  // The tagged fixture represents the known relevant and irrelevant utterances.
  pure def matchesFirstRemark(r) =
    valid(r) and includes(r, firstRemark) and disjoint(r, secondRemark)

  action init = state' = {
    clip: noClip,
    savedClip: noClip,
    savedSource: 0,
    staged: noClip,
    source: 0,
    transcript: "original",
    request: "idle",
    status: "idle",
    beforeClip: noClip,
    beforeRequest: "idle",
    last: "init",
  }

  action update(revision, event) = state' = {
    ...revision, beforeClip: state.clip, beforeRequest: state.request, last: event,
  }

  action submit = all {
    state.request == "idle" and state.clip == noClip,
    if (state.transcript == "missing")
      update({ ...state, status: "transcript-required" }, "reject")
    else
      update({ ...state, request: "pending", status: "working", staged: noClip }, "submit"),
  }

  action stage(candidate) = all {
    state.request != "idle",
    update({ ...state, staged: candidate }, "stage"),
  }

  action stageFirstRemark(candidate) = all {
    hasRelevantRemark(state.transcript) and matchesFirstRemark(candidate),
    stage(candidate),
  }

  action rejectReply(status) =
    update({ ...state, request: "idle", staged: noClip, status: status }, "reject")

  action finish(kind) = all {
    state.request != "idle",
    if (state.request == "cancelled") rejectReply("cancelled")
    else if (state.request == "stale" or state.clip != noClip) rejectReply("stale")
    else if (kind == "failed") rejectReply("failed")
    else if (kind == "no-match") all {
      not(hasRelevantRemark(state.transcript)),
      rejectReply("no-match"),
    }
    else all {
      hasRelevantRemark(state.transcript),
      if (not(valid(state.staged))) rejectReply("invalid-range")
      else update({
        ...state,
        request: "idle",
        status: "applied",
        clip: state.staged,
        staged: noClip,
      }, "apply"),
    },
  }

  action cancel = all {
    state.request == "pending",
    update({ ...state, request: "cancelled", status: "cancelled" }, "cancel"),
  }

  pure def afterChange(request) = if (request == "pending") "stale" else request

  action replaceSource = update({
    ...state,
    request: afterChange(state.request),
    source: 1 - state.source,
    clip: noClip,
  }, "replace-source")

  action replaceTranscript(version) = update({
    ...state,
    request: afterChange(state.request),
    transcript: version,
  }, "replace-transcript")

  action placeManual = all {
    state.clip == noClip,
    update({
      ...state, clip: manual, request: afterChange(state.request), status: "edited",
    }, "manual"),
  }

  action trim(candidate) = all {
    state.clip != noClip,
    if (valid(candidate))
      update({ ...state, clip: candidate, status: "edited" }, "trim")
    else
      update({ ...state, status: "invalid-range" }, "reject"),
  }

  action trimStart(start) = trim(interval(start, state.clip.end))
  action trimEnd(end) = trim(interval(state.clip.start, end))

  action save = update({ ...state, savedClip: state.clip, savedSource: state.source }, "save")

  action reopen = all {
    state.request == "idle" and state.source == state.savedSource,
    update({ ...state, clip: state.savedClip, status: "opened" }, "reopen"),
  }

  // The sole clip starts at timeline zero.
  val timelineLength = state.clip.end - state.clip.start
  def timesAt(position) = {
    val sourceTime = state.clip.start + position
    { video: sourceTime, game: sourceTime, voice: sourceTime - 2 }
  }

  action step = any {
    submit,
    stageFirstRemark(selected),
    stage(selected),
    stage(interval(-1, 40)),
    stage(interval(30, 91)),
    stage(interval(40, 40)),
    finish("complete"),
    finish("failed"),
    finish("no-match"),
    cancel,
    replaceSource,
    replaceTranscript("original"),
    replaceTranscript("replacement"),
    replaceTranscript("missing"),
    replaceTranscript("unrelated"),
    placeManual,
    trimStart(32),
    trimEnd(38),
    trimStart(-1),
    trimEnd(91),
    trimStart(40),
    trimEnd(31),
    save,
    reopen,
  }

  val workflowRules = all {
    state.clip == noClip or valid(state.clip),
    state.savedClip == noClip or valid(state.savedClip),
    Set("reject", "stage", "submit", "cancel", "save", "replace-transcript").contains(state.last)
      implies state.clip == state.beforeClip,
    (state.last == "apply") implies all {
      state.beforeRequest == "pending" and state.request == "idle",
      state.beforeClip == noClip and valid(state.clip) and state.status == "applied",
      hasRelevantRemark(state.transcript),
    },
    (state.last == "reject" and state.status == "no-match") implies
      (state.transcript != "missing" and not(hasRelevantRemark(state.transcript))),
    (state.last == "save") implies (state.savedClip == state.clip and state.savedSource == state.source),
    (state.last == "reopen") implies state.savedClip == state.clip,
  }

  // BDD: ai-clip-editing.feature:9
  run relevantUtteranceTest = init.then(submit).then(stageFirstRemark(selected))
    .then(finish("complete"))
    .expect(includes(state.clip, firstRemark) and disjoint(state.clip, secondRemark))
    .expect(not(matchesFirstRemark(interval(34, 40))) and not(matchesFirstRemark(interval(30, 63))))

  // BDD: ai-clip-editing.feature:20
  run selectedClipIsEditableTest = init.then(submit).then(stage(selected)).then(finish("complete"))
    .expect(state.status == "applied" and state.clip == selected and timelineLength == 10)
    .expect(timesAt(3) == { video: 33, game: 33, voice: 31 })
    .then(trimEnd(38)).expect(state.clip == interval(30, 38))

  // BDD: ai-clip-editing.feature:30
  run trimStartAndSaveTest = init.then(submit).then(stage(selected)).then(finish("complete"))
    .then(trimStart(32)).expect(state.clip == interval(32, 40) and timelineLength == 8)
    .expect(timesAt(1) == { video: 33, game: 33, voice: 31 })
    .then(save).then(trimEnd(38)).then(reopen)
    .expect(state.clip == interval(32, 40) and timelineLength == 8)

  // BDD: ai-clip-editing.feature:39
  run trimEndTest = init.then(submit).then(stage(selected)).then(finish("complete"))
    .then(trimEnd(38)).expect(state.clip == interval(30, 38) and timelineLength == 8)

  // BDD: ai-clip-editing.feature:44 [51, 52, 53, 54]
  run invalidGuiTrimExamplesTest = init.then(submit).then(stage(selected)).then(finish("complete"))
    .then(trimStart(32))
    .then(trimStart(-1)).expect(state.clip == interval(32, 40) and state.status == "invalid-range")
    .then(trimEnd(91)).expect(state.clip == interval(32, 40) and state.status == "invalid-range")
    .then(trimStart(40)).expect(state.clip == interval(32, 40) and state.status == "invalid-range")
    .then(trimEnd(31)).expect(state.clip == interval(32, 40) and state.status == "invalid-range")

  // BDD: ai-clip-editing.feature:56 [65, 66]
  run noSelectionReasonsTest = init.then(replaceTranscript("missing")).then(submit)
    .expect(state.clip == noClip and state.status == "transcript-required" and state.request == "idle")
    .then(replaceTranscript("unrelated")).then(submit)
    .expect(state.request == "pending" and remarks(state.transcript) == Set(secondRemark))
    .then(finish("no-match"))
    .expect(state.clip == noClip and state.status == "no-match")

  // BDD: ai-clip-editing.feature:68 [77, 78, 79]
  run invalidAiReplyExamplesTest = init
    .then(submit).then(stage(interval(-1, 40))).then(finish("complete"))
    .expect(state.status == "invalid-range" and state.clip == noClip)
    .then(submit).then(stage(interval(30, 91))).then(finish("complete"))
    .expect(state.status == "invalid-range" and state.clip == noClip)
    .then(submit).then(stage(interval(40, 40))).then(finish("complete"))
    .expect(state.status == "invalid-range" and state.clip == noClip)

  // BDD: ai-clip-editing.feature:81
  run failedAiPreservesEmptyTimelineTest = init.then(submit)
    .expect(state.status == "working" and state.clip == noClip)
    .then(stage(selected)).expect(state.status == "working" and state.clip == noClip)
    .then(finish("failed")).expect(state.status == "failed" and state.clip == noClip)

  // BDD: ai-clip-editing.feature:87
  run lateResultAfterCancellationTest = init.then(submit).then(cancel)
    .then(stage(selected)).then(finish("complete"))
    .expect(state.clip == noClip and state.status == "cancelled" and state.request == "idle")

  // BDD: ai-clip-editing.feature:93 [102, 103, 104]
  run changedPreconditionsExamplesTest = init.then(submit).then(replaceSource)
    .then(stage(selected)).then(finish("complete"))
    .expect(state.source == 1 and state.clip == noClip and state.status == "stale")
    .then(submit).then(replaceTranscript("replacement")).then(stage(selected)).then(finish("complete"))
    .expect(state.transcript == "replacement" and state.clip == noClip and state.status == "stale")
    .then(submit).then(placeManual).then(stage(selected)).then(finish("complete"))
    .expect(state.clip == manual and state.status == "stale")
}
