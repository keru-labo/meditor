module FilterGraph {
  type Edge = { from: str, to: str }
  type Catalog = { filters: Set[str], encoders: Set[str], formats: Set[str] }
  type Configuration = { filter: str, target: str, value: int, encoder: str, quality: int, format: str }
  type State = {
    available: Catalog,
    screen: bool,
    settings: Configuration,
    normalization: str -> int,
    graph: Set[Edge],
    beforeRejected: Set[Edge],
    rejected: List[Edge],
  }

  var state: State

  pure val full = { filters: Set("loudnorm", "scale", "overlay", "amix"),
    encoders: Set("h264", "aac"), formats: Set("mp4", "mkv") }
  pure val minimal = { filters: Set("scale"), encoders: Set("h264"), formats: Set("mp4") }
  pure val emptyCatalog = { filters: Set(), encoders: Set(), formats: Set() }
  pure val defaultSettings = {
    filter: "", target: "", value: 0, encoder: "h264", quality: 20, format: "mp4",
  }
  pure def edge(source, target) = { from: source, to: target }
  pure val initialGraph = Set(edge("video", "video.out"), edge("speakerA", "audio.out"))
  pure val compositeInputs = Set(
    edge("video", "composite.main"), edge("video", "zoom.in"),
    edge("zoom", "composite.overlay"),
  )
  pure val compositeOutput = edge("composite", "video.out")
  pure val pictureInPicture = compositeInputs.union(Set(compositeOutput))
  pure val arrangedAudio = Set(
    edge("game", "gameGain.in"), edge("gameGain", "mix.game"),
    edge("speakerA", "speakerGain.in"), edge("speakerGain", "mix.voice"),
    edge("mix", "audio.out"),
  )

  pure val videoSources = Set("video", "zoom", "composite")
  pure val audioSources = Set("game", "speakerA", "speakerB", "gameGain", "speakerGain", "voiceFilter", "mix")
  pure val videoInputs = Set("zoom.in", "composite.main", "composite.overlay", "video.out")
  pure val audioInputs = Set("gameGain.in", "speakerGain.in", "voiceFilter.in", "mix.game", "mix.voice", "audio.out")
  pure def compatible(link) =
    (videoSources.contains(link.from) and videoInputs.contains(link.to))
      or (audioSources.contains(link.from) and audioInputs.contains(link.to))

  pure def replaceInput(graph, link) = graph.exclude(graph.filter(old => old.to == link.to)).union(Set(link))
  pure def uniqueInputs(graph) =
    graph.forall(link => graph.filter(other => link.to == other.to).size() == 1)
  // Ordered processing layers keep this bounded graph acyclic.
  pure def sourceRank(source) =
    if (Set("video", "game", "speakerA", "speakerB").contains(source)) 0
    else if (Set("zoom", "gameGain", "speakerGain").contains(source)) 1
    else if (Set("composite", "voiceFilter").contains(source)) 2
    else 3
  pure def inputRank(target) =
    if (Set("zoom.in", "gameGain.in", "speakerGain.in").contains(target)) 1
    else if (Set("composite.main", "composite.overlay", "voiceFilter.in").contains(target)) 2
    else if (Set("mix.game", "mix.voice").contains(target)) 3
    else 4
  pure def validGraph(graph) = uniqueInputs(graph)
    and graph.forall(link => compatible(link) and sourceRank(link.from) < inputRank(link.to))
    and (graph.contains(compositeOutput) implies compositeInputs.subseteq(graph))
  pure def validSettings(catalog, settings) =
    catalog.filters.contains(settings.filter)
      and catalog.encoders.contains(settings.encoder)
      and catalog.formats.contains(settings.format)
      and 0 <= settings.quality and settings.quality <= 51
      and (if (settings.filter == "loudnorm")
        Set("speakerA", "speakerB").contains(settings.target) and -70 <= settings.value and settings.value <= -5
      else if (settings.filter == "amix") settings.target == "mix" and settings.value > 0
      else settings.target == "video" and settings.value > 0)

  action start(catalog) = state' = {
    available: catalog,
    screen: false,
    settings: defaultSettings,
    normalization: Map("speakerA" -> 0, "speakerB" -> -20),
    graph: initialGraph,
    beforeRejected: Set(),
    rejected: [],
  }

  action init = any { start(full), start(minimal) }

  action openSettings = state' = { ...state, screen: true }

  action configure(settings) = all {
    state.screen,
    validSettings(state.available, settings),
    state' = { ...state, settings: settings },
  }

  action chatNormalize(track, target) = all {
    validSettings(state.available, { ...defaultSettings, filter: "loudnorm", target: track, value: target }),
    state' = { ...state, normalization: state.normalization.set(track, target) },
  }

  action guiNormalize(track, target) = all {
    state.screen,
    state.normalization.get(track) != 0,
    chatNormalize(track, target),
  }

  action buildAudio = all {
    state.available.filters.contains("amix"),
    state' = { ...state, graph: state.graph.exclude(state.graph.filter(link => audioInputs.contains(link.to)))
      .union(arrangedAudio), rejected: [] },
  }

  action addVoiceFilter = all {
    state.graph.contains(edge("speakerGain", "mix.voice")),
    state' = { ...state, graph: replaceInput(state.graph, edge("voiceFilter", "mix.voice"))
      .union(Set(edge("speakerGain", "voiceFilter.in"))), rejected: [] },
  }

  action connect(link) = {
    val candidate = replaceInput(state.graph, link)
    if (validGraph(candidate)) state' = { ...state, graph: candidate, rejected: [] }
    else state' = { ...state, beforeRejected: state.graph, rejected: [link] }
  }

  action connectVideo(link) = all {
    state.available.filters.contains("overlay"),
    state.available.filters.contains("scale"),
    connect(link),
  }

  action baseIntoComposite = connectVideo(edge("video", "composite.main"))
  action sourceIntoZoom = connectVideo(edge("video", "zoom.in"))
  action zoomIntoOverlay = connectVideo(edge("zoom", "composite.overlay"))
  action compositeIntoOutput = connectVideo(compositeOutput)

  pure def previewVideo(graph) =
    if (pictureInPicture.subseteq(graph)) ["original", "zoomed overlay"]
    else if (graph.contains(edge("video", "video.out"))) ["original"]
    else []

  pure def voicePath(graph) =
    if (Set(edge("speakerA", "speakerGain.in"), edge("speakerGain", "voiceFilter.in"),
      edge("voiceFilter", "mix.voice"), edge("mix", "audio.out")).subseteq(graph))
      ["speakerA", "gain", "filter", "mix"]
    else if (Set(edge("speakerA", "speakerGain.in"), edge("speakerGain", "mix.voice"),
      edge("mix", "audio.out")).subseteq(graph)) ["speakerA", "gain", "mix"]
    else []

  pure def gamePath(graph) =
    if (Set(edge("game", "gameGain.in"), edge("gameGain", "mix.game"),
      edge("mix", "audio.out")).subseteq(graph)) ["game", "gain", "mix"]
    else []

  pure val selectedSettings = {
    filter: "scale", target: "video", value: 2, encoder: "h264", quality: 18, format: "mkv",
  }
  val displayedCatalog = if (state.screen) state.available else emptyCatalog

  action step = any {
    openSettings,
    configure(selectedSettings),
    configure({ ...selectedSettings, format: "mp4" }),
    chatNormalize("speakerA", -16),
    guiNormalize("speakerA", -18),
    baseIntoComposite,
    sourceIntoZoom,
    zoomIntoOverlay,
    compositeIntoOutput,
    buildAudio,
    addVoiceFilter,
    connect(edge("video", "voiceFilter.in")),
    connect(edge("voiceFilter", "speakerGain.in")),
  }

  val workflowRules = all {
    validGraph(state.graph),
    previewVideo(state.graph).length() > 0,
    state.normalization.get("speakerB") == -20,
    state.settings.filter == "" or validSettings(state.available, state.settings),
    state.rejected.foldl(true, (ok, link) => ok and not(validGraph(replaceInput(state.beforeRejected, link)))
      and state.graph == state.beforeRejected),
  }

  // BDD: ffmpeg-controls.feature:4
  run showsOnlyAvailableOptionsTest = start(full).expect(displayedCatalog == emptyCatalog).then(openSettings)
    .expect(displayedCatalog == full)
    .then(configure(selectedSettings))
    .expect(state.settings == selectedSettings and workflowRules)
  // BDD: ffmpeg-controls.feature:4
  run anotherDistributionOffersItsOwnOptionsTest = start(minimal).then(openSettings)
    .expect(displayedCatalog == minimal and not(validSettings(displayedCatalog, selectedSettings)))
    .then(configure({ ...selectedSettings, format: "mp4" }))
    .expect(state.settings.format == "mp4" and workflowRules)

  // BDD: ffmpeg-controls.feature:11
  run chatAndGuiShareSpeakerSettingsTest = start(full).then(chatNormalize("speakerA", -16)).then(openSettings)
    .expect(state.screen and state.normalization.get("speakerA") == -16
      and state.normalization.get("speakerB") == -20)
    .then(guiNormalize("speakerA", -18))
    .expect(state.normalization.get("speakerA") == -18 and workflowRules)

  // BDD: ffmpeg-controls.feature:18
  run splitAndComposePictureTest = start(full).then(compositeIntoOutput)
    .expect(state.rejected == [compositeOutput] and previewVideo(state.graph) == ["original"])
    .then(baseIntoComposite).then(sourceIntoZoom)
    .then(zoomIntoOverlay).then(compositeIntoOutput)
    .expect(state.rejected == [] and state.graph.filter(link => link.from == "video").size() == 2
      and previewVideo(state.graph) == ["original", "zoomed overlay"] and workflowRules)

  // BDD: ffmpeg-controls.feature:25
  run addFilterOnlyToVoiceBranchTest = start(full).then(buildAudio)
    .expect(voicePath(state.graph) == ["speakerA", "gain", "mix"]
      and gamePath(state.graph) == ["game", "gain", "mix"])
    .then(addVoiceFilter)
    .expect(voicePath(state.graph) == ["speakerA", "gain", "filter", "mix"]
      and gamePath(state.graph) == ["game", "gain", "mix"] and workflowRules)

  // BDD: ffmpeg-controls.feature:31
  run rejectsIncompatibleConnectionsWithoutBreakingPreviewTest = start(full).then(buildAudio)
    .then(baseIntoComposite).then(sourceIntoZoom).then(zoomIntoOverlay).then(compositeIntoOutput)
    .then(connect(edge("video", "voiceFilter.in")))
    .expect(state.rejected == [edge("video", "voiceFilter.in")] and workflowRules
      and previewVideo(state.graph) == ["original", "zoomed overlay"]
      and gamePath(state.graph) == ["game", "gain", "mix"]
      and voicePath(state.graph) == ["speakerA", "gain", "mix"])
}
