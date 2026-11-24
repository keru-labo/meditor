// Finite model of subtitle render plans. It compares text, style and composition
// instructions before rasterization or MP4 encoding. The ASCII cue tokens
// stand for the BDD's distinct Japanese bodies; TLC cannot parse Unicode strings.
module SubtitleWorkflow {
  type Style = { font: str, size: int, color: str, edge: str, outline: int, background: str }
  type Cue = { speaker: str, body: str, start: int, end: int, style: Style }
  type Project = { cues: List[Cue], style: Style, speakerAColor: str }
  type Session = { project: Project, undo: List[Project] }
  type State = { live: Session, archive: List[Session], fonts: Set[str], fixture: str }

  var state: State

  pure val defaults = { font: "base", size: 24, color: "white",
    edge: "black", outline: 2, background: "none" }
  pure val inherit = { font: "", size: 0, color: "",
    edge: "", outline: -1, background: "" }
  pure def cue(speaker, body, start, end) =
    { speaker: speaker, body: body, start: start, end: end, style: inherit }

  pure def project(cues, style) = { cues: cues, style: style, speakerAColor: "" }

  action start(fixture, projectValue, fonts) = state' = {
    live: { project: projectValue, undo: [] },
    archive: [],
    fonts: fonts,
    fixture: fixture,
  }

  action startTiming = start("timing", project([cue("A", "boss", 4, 6)], defaults),
    Set("base", "alternative"))
  action startOverlap = start("overlap",
    project([cue("A", "lets-go", 4, 6), cue("B", "wait", 4, 6)], defaults),
    Set("base", "alternative"))
  action startStyles = start("styles",
    project([cue("A", "first", 4, 6), cue("A", "second", 4, 6)], defaults),
    Set("base", "alternative"))
  action startSpeakers = start("speaker",
    project([cue("A", "first", 4, 6), cue("B", "second", 4, 6),
      { ...cue("A", "third", 4, 6), style: { ...inherit, color: "red" } }], defaults),
    Set("base", "alternative"))
  action startFallback = start("fallback",
    project([cue("A", "first", 4, 6)], { ...defaults, font: "unavailable" }), Set("base"))

  action init = any { startTiming, startOverlap, startStyles, startSpeakers, startFallback }

  pure def resolvedStyle(projectValue, entry) = {
    val own = entry.style
    val common = projectValue.style
    val speakerColor = if (entry.speaker == "A") projectValue.speakerAColor else ""
    {
      font: if (own.font == "") common.font else own.font,
      size: if (own.size == 0) common.size else own.size,
      color: if (own.color != "") own.color
        else if (speakerColor != "") speakerColor else common.color,
      edge: if (own.edge == "") common.edge else own.edge,
      outline: if (own.outline < 0) common.outline else own.outline,
      background: if (own.background == "") common.background else own.background,
    }
  }

  pure def actualFont(requested, fonts) = if (fonts.contains(requested)) requested else "base"

  pure def renderCue(projectValue, entry, fonts) = {
    val style = resolvedStyle(projectValue, entry)
    {
      text: entry.body,
      font: actualFont(style.font, fonts),
      size: style.size,
      color: style.color,
      edge: style.edge,
      outline: style.outline,
      backing: style.background,
    }
  }

  pure def renderPlan(projectValue, second, fonts) =
    projectValue.cues.foldl([], (rendered, entry) =>
      if (entry.start <= second and second < entry.end)
        rendered.append(renderCue(projectValue, entry, fonts)) else rendered)

  pure def mapList(items, projection) =
    items.foldl([], (result, entry) => result.append(projection(entry)))

  pure def allList(items, predicate) =
    items.foldl(true, (result, entry) => result and predicate(entry))

  val liveProject = state.live.project
  def previewAt(second) = renderPlan(liveProject, second, state.fonts)
  def burnInAt(second) = renderPlan(liveProject, second, state.fonts)
  pure def backingComposite(plan, sourceColor) =
    plan.foldl(sourceColor, (visibleColor, glyph) =>
      if (glyph.backing == "none") visibleColor else glyph.backing)
  val speakerList = mapList(liveProject.cues, entry => entry.speaker)
  val requestedFont = resolvedStyle(liveProject, liveProject.cues[0]).font
  val fontNotice = if (state.fonts.contains(requestedFont)) "" else actualFont(requestedFont, state.fonts)

  action changeProject(updated) =
    if (updated == liveProject) state' = state else state' = {
      ...state,
      live: { project: updated, undo: [liveProject] },
    }

  action replaceFirst(entry) = changeProject({
    ...liveProject,
    cues: [entry].concat(liveProject.cues.slice(1, liveProject.cues.length())),
  })

  action retime(body, second) = all {
    liveProject.cues[0].end > second,
    replaceFirst({ ...liveProject.cues[0], body: body, start: second }),
  }

  action setIndividualStyle = replaceFirst({ ...liveProject.cues[0],
    style: { ...inherit, font: "alternative", size: 40, edge: "blue", outline: 6,
      background: "translucent-white" } })

  action removeIndividualEffects = {
    val entry = liveProject.cues[0]
    replaceFirst({ ...entry, style: { ...entry.style, outline: 0, background: "none" } })
  }

  action setSpeakerColor(color) = changeProject({ ...liveProject, speakerAColor: color })
  action setProjectStyle = changeProject({
    ...liveProject, style: { ...liveProject.style, color: "yellow", edge: "blue", outline: 6 },
  })

  action save = state' = { ...state, archive: [state.live] }
  action reopen = all {
    state.archive.length() == 1,
    state' = { ...state, live: state.archive[0] },
  }
  action undo = all {
    state.live.undo.length() == 1,
    state' = { ...state, live: { project: state.live.undo[0], undo: [] } },
  }

  action step = any {
    all { state.fixture == "timing", any { retime("boss-battle", 5), retime("boss", 4) } },
    all { state.fixture == "styles", any { setIndividualStyle, removeIndividualEffects, setProjectStyle, undo } },
    all { state.fixture == "speaker", any {
      setSpeakerColor("yellow"), setSpeakerColor("green"), save, reopen, undo,
    } },
    // Rendering the unchanged fixture is permitted too.
    state' = state,
  }

  val workflowRules = all {
    liveProject.cues.length() > 0,
    state.live.undo.length() <= 1,
    state.archive.length() <= 1,
    allList(liveProject.cues, entry => {
      val rendering = renderCue(liveProject, entry, state.fonts)
      all {
        entry.start < entry.end,
        rendering.text == entry.body,
        state.fonts.contains(rendering.font),
        rendering.outline >= 0,
        (entry.style.color != "") implies rendering.color == entry.style.color,
        (entry.style.background == "" and liveProject.style.background == "none")
          implies rendering.backing == "none",
      }
    }),
    Set(3, 4, 5, 6, 7).forall(second => {
      val expected = liveProject.cues.foldl(0, (count, entry) =>
        if (second < entry.start or entry.end <= second) count else count + 1)
      previewAt(second).length() == expected and burnInAt(second) == previewAt(second)
    }),
    not(state.fonts.contains(requestedFont)) implies fontNotice == previewAt(4)[0].font,
  }

  // BDD: subtitle-editing.feature:4
  run retimesAndEditsTextTest = startTiming.then(retime("boss-battle", 5))
    .expect(previewAt(4) == [] and mapList(previewAt(5), glyph => glyph.text) == ["boss-battle"])

  // BDD: subtitle-editing.feature:10
  run rendersBodiesAndListsSpeakersTest = startOverlap.expect(all {
    mapList(previewAt(4), glyph => glyph.text) == ["lets-go", "wait"],
    mapList(burnInAt(4), glyph => glyph.text) == ["lets-go", "wait"],
    speakerList == ["A", "B"],
  })

  // BDD: subtitle-editing.feature:16
  run leavesBackgroundVisibleTest = startOverlap.expect(all {
    previewAt(4).length() == 2,
    allList(previewAt(4), glyph => glyph.text != "" and glyph.outline == 2 and glyph.backing == "none"),
    burnInAt(4) == previewAt(4),
    Set("white", "blue").forall(sourceColor =>
      backingComposite(previewAt(4), sourceColor) == sourceColor
        and backingComposite(burnInAt(4), sourceColor) == sourceColor),
  })

  // BDD: subtitle-editing.feature:22
  run appliesAndRemovesIndividualEffectsTest = startStyles.then(setIndividualStyle)
    .expect(all {
      previewAt(4)[0] == { text: "first", font: "alternative", size: 40, color: "white",
        edge: "blue", outline: 6, backing: "translucent-white" },
      previewAt(4)[1] == { text: "second", font: "base", size: 24, color: "white",
        edge: "black", outline: 2, backing: "none" },
      burnInAt(4) == previewAt(4),
    }).then(removeIndividualEffects).expect(all {
      previewAt(4)[0].outline == 0,
      previewAt(4)[0].backing == "none",
      previewAt(4)[0].font == "alternative",
      previewAt(4)[0].size == 40,
      previewAt(4)[1].font == "base",
      burnInAt(4) == previewAt(4),
    })

  // BDD: subtitle-editing.feature:29
  run restoresSpeakerColorsAndOverridesTest = startSpeakers.then(setSpeakerColor("yellow"))
    .expect(mapList(previewAt(4), glyph => glyph.color) == ["yellow", "white", "red"])
    .then(save).then(setSpeakerColor("green"))
    .expect(mapList(previewAt(4), glyph => glyph.color) == ["green", "white", "red"])
    .then(reopen)
    .expect(mapList(previewAt(4), glyph => glyph.color) == ["yellow", "white", "red"])
    .then(undo)
    .expect(mapList(previewAt(4), glyph => glyph.color) == ["white", "white", "red"])

  // BDD: subtitle-editing.feature:37
  run reportsSharedFallbackTest = startFallback.expect(all {
    fontNotice == "base",
    previewAt(4)[0].font == fontNotice,
    burnInAt(4)[0].font == fontNotice,
  })

  // BDD: subtitle-editing.feature:43
  run appliesAndUndoesProjectStyleTest = startStyles.then(setProjectStyle)
    .expect(all {
      allList(previewAt(4), glyph => glyph.color == "yellow" and glyph.edge == "blue"
        and glyph.outline == 6 and glyph.backing == "none"),
      burnInAt(4) == previewAt(4),
    }).then(undo)
    .expect(allList(previewAt(4), glyph => glyph.color == "white" and glyph.edge == "black"
      and glyph.outline == 2 and glyph.backing == "none") and burnInAt(4) == previewAt(4))
}
