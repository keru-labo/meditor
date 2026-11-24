module MediaIntake {
  type Source = NoneLoaded | EmbeddedTracks | ExternalTracks | MixedTrack
  type Track = Game | SpeakerA | SpeakerB | OriginalMix
  type Separation = Idle | LocalRunning | Separated | Failed
  type Offsets = { game: int, speakerA: int, speakerB: int }

  type State = {
    source: Source,
    videoReferenced: bool,
    tracks: Set[Track],
    originalReferenced: bool,
    offsets: Offsets,
    separation: Separation,
    reason: str,
    muteB: bool,
  }

  var state: State

  pure val separateTracks = Set(Game, SpeakerA, SpeakerB)
  pure val zeroOffsets = { game: 0, speakerA: 0, speakerB: 0 }
  pure val empty = {
    source: NoneLoaded,
    videoReferenced: false,
    tracks: Set(),
    originalReferenced: false,
    offsets: zeroOffsets,
    separation: Idle,
    reason: "",
    muteB: false,
  }

  action init = state' = empty

  action load(source) = state' = {
    ...empty,
    source: source,
    videoReferenced: true,
    tracks: if (source == MixedTrack) Set(OriginalMix) else separateTracks,
    originalReferenced: source == MixedTrack,
  }

  action offsetTrack(track, offset) = all {
    state.source == ExternalTracks,
    separateTracks.contains(track),
    val offsets = state.offsets
    val updated = match track {
      | Game => { ...offsets, game: offset }
      | SpeakerA => { ...offsets, speakerA: offset }
      | SpeakerB => { ...offsets, speakerB: offset }
      | OriginalMix => offsets
    }
    state' = { ...state, offsets: updated },
  }

  action startLocalSeparation = all {
    state.source == MixedTrack,
    state.separation == Idle or state.separation == Failed,
    state' = { ...state, separation: LocalRunning, reason: "" },
  }

  action finishSeparation(success) = all {
    state.separation == LocalRunning,
    state' = if (success) {
      { ...state, tracks: separateTracks, separation: Separated }
    } else {
      { ...state, separation: Failed, reason: "separation failed" }
    },
  }

  action setMuteB(muted) = all {
    state.tracks.contains(SpeakerB),
    state' = { ...state, muteB: muted },
  }

  val audible = if (state.muteB) state.tracks.filter(track => track != SpeakerB) else state.tracks

  action step = any {
    load(EmbeddedTracks),
    load(ExternalTracks),
    load(MixedTrack),
    offsetTrack(Game, -2),
    offsetTrack(Game, 2),
    offsetTrack(SpeakerA, -2),
    offsetTrack(SpeakerA, 2),
    offsetTrack(SpeakerB, -2),
    offsetTrack(SpeakerB, 2),
    startLocalSeparation,
    finishSeparation(true),
    finishSeparation(false),
    setMuteB(true),
    setMuteB(false),
  }

  val workflowRules = all {
    state.videoReferenced == (state.source != NoneLoaded),
    state.originalReferenced == (state.source == MixedTrack),
    (state.source != ExternalTracks) implies state.offsets == zeroOffsets,
    (state.source == EmbeddedTracks or state.source == ExternalTracks) implies
      state.tracks == separateTracks,
    (state.separation == LocalRunning or state.separation == Failed) implies all {
      state.source == MixedTrack,
      Set(OriginalMix).subseteq(audible),
    },
    (state.separation == Separated) implies all {
      state.source == MixedTrack,
      state.tracks == separateTracks,
      state.originalReferenced,
    },
    (state.separation == Failed) == (state.reason != ""),
    state.muteB implies (not(audible.contains(SpeakerB)) and Set(Game, SpeakerA).subseteq(audible)),
  }

  // BDD: media-import.feature:5 [13]
  run embeddedImportedTracksTest = init.then(load(EmbeddedTracks))
    .expect(state.videoReferenced and Set(Game, SpeakerA).subseteq(state.tracks))

  // BDD: media-import.feature:5 [14]
  run externalImportedTracksTest = init.then(load(ExternalTracks))
    .then(offsetTrack(SpeakerA, 2))
    .expect(state.offsets == { game: 0, speakerA: 2, speakerB: 0 })
    .then(offsetTrack(Game, -2)).then(offsetTrack(SpeakerB, 2))
    .expect(state.videoReferenced and Set(Game, SpeakerA, SpeakerB).subseteq(state.tracks)
      and state.offsets == { game: -2, speakerA: 2, speakerB: 2 })

  // BDD: media-import.feature:5 [15]
  run originalMixImportedTest = init.then(load(MixedTrack))
    .expect(state.videoReferenced and state.tracks == Set(OriginalMix) and state.originalReferenced)

  // BDD: media-import.feature:17
  run localSeparationAndMuteTest = init.then(load(MixedTrack)).then(startLocalSeparation)
    .expect(state.separation == LocalRunning)
    .then(finishSeparation(true))
    .expect(state.tracks == separateTracks and state.originalReferenced)
    .then(setMuteB(true))
    .expect(audible == Set(Game, SpeakerA) and state.originalReferenced)

  // BDD: media-import.feature:25
  run separationFailureKeepsOriginalTest = init.then(load(MixedTrack)).then(startLocalSeparation)
    .then(finishSeparation(false))
    .expect(state.reason != "" and state.originalReferenced and audible == Set(OriginalMix))
}
