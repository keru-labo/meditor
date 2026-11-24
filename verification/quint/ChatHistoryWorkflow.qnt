module ChatHistoryWorkflow {
  type Scene = Boss | Highlight | Waiting
  type Prompt =
    BossCut | Highlights | TrimWaiting | ReorderAndRaise | YellowText |
    ZoomScene | InsertSound | Portrait | CutAndGain | AdjustBoss
  type Gain = Normal | Raised | InvalidGain
  type Author = Ai | Human
  type Channel = Gui | Chat
  type Change = Cut | Caption | Presentation | Compound | SelectiveRevert
  type Outcome = NoOutcome | Applied | InvalidVolume | Discarded
  type ClipRange = { start: int, end: int }
  type Document = {
    clips: List[Scene],
    captions: Set[Scene],
    bossCaption: bool,
    bossRange: ClipRange,
    highlightCaption: bool,
    gain: Gain,
    yellowText: bool,
    zoomAt: List[Scene],
    soundAt: int,
    portrait: bool,
  }
  type Audit = {
    author: Author, change: Change, target: int,
    oldClips: List[Scene], newClips: List[Scene], oldBossRange: ClipRange, newBossRange: ClipRange,
  }
  type Turn = { prompt: Prompt, outcome: Outcome, selected: List[Scene] }
  type Session = {
    versions: List[Document],
    cursor: int,
    audit: List[Audit],
    conversation: List[Turn],
  }
  type Request = Idle | Active(Prompt) | Cancelled(Prompt)
  type ImpactNotice = { target: int, removedCaptions: Set[Scene] }
  type Decision = NoDecision | Awaiting(ImpactNotice)
  type Event = Start | Submitted | Cancel | Reply | Manual | Undo(Channel)
    | Redo(Channel) | Saved | Reopened | Impact | Confirmed | Declined | Reverted
  type State = {
    session: Session,
    previous: Session,
    archive: List[Session],
    priorArchive: List[Session],
    request: Request,
    decision: Decision,
    event: Event,
    outcome: Outcome,
    ticks: int,
  }

  var state: State

  pure val source = [Boss, Waiting, Highlight]
  pure val speech = Set(Boss, Highlight)
  pure val originalBossRange = { start: 30, end: 40 }
  pure val adjustedBossRange = { start: 32, end: 38 }
  pure val emptyDocument = {
    clips: [], captions: Set(), bossCaption: false, bossRange: originalBossRange,
    highlightCaption: false, gain: Normal,
    yellowText: false, zoomAt: [], soundAt: -1, portrait: false,
  }
  pure val initialSession = { versions: [emptyDocument], cursor: 0, audit: [], conversation: [] }
  action init = state' = {
    session: initialSession, previous: initialSession, archive: [], priorArchive: [],
    request: Idle, decision: NoDecision, event: Start, outcome: NoOutcome, ticks: 0,
  }

  pure def document(session) = session.versions[session.cursor]
  pure def filterScenes(scenes, keep) = scenes.foldl([], (result, scene) =>
    if (keep(scene)) result.append(scene) else result)
  pure def hasScene(scenes, target) = scenes.foldl(false, (found, scene) => found or scene == target)
  pure def listMap(values, transform) = values.foldl([], (result, value) => result.append(transform(value)))
  pure def lastEntry(values) = values[values.length() - 1]
  pure def undoAvailable(session) = session.cursor > 0
  pure def redoAvailable(session) = session.cursor + 1 < session.versions.length()

  // Preview derives its order and presentation settings from the editable document.
  pure def preview(doc) = {
    clips: doc.clips,
    visibleCaptions: filterScenes(doc.clips, scene => doc.captions.contains(scene)),
    correctedBoss: doc.bossCaption,
    // This token denotes the BDD's caption text "ボス戦".
    bossCaptionText: if (doc.bossCaption) "boss-battle" else "",
    bossRange: doc.bossRange,
    correctedHighlight: doc.highlightCaption,
    gain: doc.gain,
    textColor: if (doc.yellowText) "yellow" else "default",
    zoomAt: filterScenes(doc.zoomAt, scene => hasScene(doc.clips, scene)),
    soundAt: doc.soundAt,
    aspect: if (doc.portrait) "9:16" else "16:9",
  }

  pure def appendChange(session, nextDocument, author, change, target) = {
    val destination = session.cursor + 1
    {
      ...session,
      versions: session.versions.slice(0, destination).append(nextDocument),
      cursor: destination,
      audit: session.audit.append({
        author: author, change: change, target: target,
        oldClips: document(session).clips, newClips: nextDocument.clips,
        oldBossRange: document(session).bossRange, newBossRange: nextDocument.bossRange,
      }),
    }
  }

  action record(nextSession, event, request, decision, outcome, archive) = state' = {
    session: nextSession, previous: state.session, archive: archive, priorArchive: state.archive,
    request: request, decision: decision, event: event, outcome: outcome,
    ticks: state.ticks + 1,
  }

  action submit(prompt) = all {
    state.request == Idle,
    state.decision == NoDecision,
    state.session.conversation.length() < 3,
    record(state.session, Submitted, Active(prompt), NoDecision, NoOutcome, state.archive),
  }

  action cancel = match state.request {
    | Active(prompt) => record(state.session, Cancel, Cancelled(prompt), NoDecision, NoOutcome, state.archive)
    | Idle => all { false, state' = state }
    | Cancelled(_) => all { false, state' = state }
  }

  pure def editPlan(doc, prompt, gain) = match prompt {
    | BossCut => { ...doc, clips: filterScenes(source, scene => scene == Boss) }
    | Highlights => { ...doc, clips: filterScenes(source, scene => speech.contains(scene)) }
    | TrimWaiting => {
        val edited = filterScenes(source, scene => scene != Waiting)
        { ...doc, clips: edited, captions: speech }
      }
    | ReorderAndRaise => { ...doc, clips: [doc.clips[1], doc.clips[0]], gain: Raised }
    | YellowText => { ...doc, yellowText: true }
    | ZoomScene => { ...doc, zoomAt: [doc.clips[0]] }
    | InsertSound => { ...doc, soundAt: 1 }
    | Portrait => { ...doc, portrait: true }
    | CutAndGain => { ...doc, clips: filterScenes(source, scene => scene == Boss), gain: gain }
    | AdjustBoss => { ...doc, bossRange: adjustedBossRange }
  }

  pure def changeOf(prompt) = match prompt {
    | BossCut => Cut
    | Highlights => Cut
    | TrimWaiting => Cut
    | AdjustBoss => Cut
    | ReorderAndRaise => Compound
    | CutAndGain => Compound
    | _ => Presentation
  }

  action reply(gain) = match state.request {
    | Active(prompt) => {
        val doc = document(state.session)
        all {
          state.session.audit.length() < 4,
          // The same validation gates the entire proposed batch.
          if (prompt == ReorderAndRaise)
            doc.clips.length() == 2 and state.session.conversation.length() > 0
          else true,
          if (prompt == AdjustBoss) hasScene(doc.clips, Boss) else true,
          if (changeOf(prompt) == Presentation) doc.clips.length() > 0 else true,
          if (prompt != CutAndGain) gain == Normal else true,
          {
            val proposal = editPlan(doc, prompt, gain)
            val accepted = proposal.gain != InvalidGain
            val outcome = if (accepted) Applied else InvalidVolume
            val turn = { prompt: prompt, outcome: outcome, selected: if (accepted) proposal.clips else [] }
            val nextSession = if (accepted and proposal != doc)
              appendChange(state.session, proposal, Ai, changeOf(prompt), -1)
              else state.session
            record({ ...nextSession, conversation: nextSession.conversation.append(turn) },
              Reply, Idle, NoDecision, outcome, state.archive)
          },
        }
      }
    | Cancelled(prompt) => all {
        gain == Normal,
        record(state.session, Reply, Idle, NoDecision, Discarded, state.archive),
      }
    | Idle => all { false, state' = state }
  }

  action manualCaption(scene) = {
    val doc = document(state.session)
    val nextDocument = if (scene == Boss)
      { ...doc, bossCaption: not(doc.bossCaption) }
      else { ...doc, highlightCaption: not(doc.highlightCaption) }
    all {
      state.decision == NoDecision,
      state.session.audit.length() < 4,
      if (scene == Boss) hasScene(doc.clips, Boss) else true,
      record(appendChange(state.session, nextDocument, Human, Caption, -1),
        Manual, state.request, NoDecision, NoOutcome, state.archive),
    }
  }

  action moveHistory(channel, backwards) = all {
    state.request == Idle,
    state.decision == NoDecision,
    if (backwards) undoAvailable(state.session) else redoAvailable(state.session),
    {
      val offset = if (backwards) -1 else 1
      val event = if (backwards) Undo(channel) else Redo(channel)
      record({ ...state.session, cursor: state.session.cursor + offset }, event,
        Idle, NoDecision, NoOutcome, state.archive)
    },
  }

  action save = all {
    state.request == Idle,
    state.decision == NoDecision,
    record(state.session, Saved, Idle, NoDecision, NoOutcome, [state.session]),
  }

  action reopen = all {
    state.request == Idle,
    state.decision == NoDecision,
    state.archive.length() == 1,
    record(state.archive[0], Reopened, Idle, NoDecision, NoOutcome, state.archive),
  }

  pure def eligibleCut(session, target) =
    target >= 0 and target < session.audit.length() and session.audit[target].change == Cut

  pure def reversion(session, target) = {
    val doc = document(session)
    val cut = session.audit[target]
    val priorClips = cut.oldClips
    val nextDocument = {
      ...doc, clips: priorClips, bossRange: cut.oldBossRange,
      bossCaption: doc.bossCaption and hasScene(priorClips, Boss),
    }
    appendChange(session, nextDocument, Human, SelectiveRevert, target)
  }

  pure def removedCaptions(before, after) =
    if (before.bossCaption and not(after.bossCaption)) Set(Boss) else Set()

  action askRevert(target) = all {
    state.request == Idle,
    state.decision == NoDecision,
    state.session.audit.length() < 4,
    eligibleCut(state.session, target),
    {
      val cut = state.session.audit[target]
      val current = document(state.session)
      val reverted = reversion(state.session, target)
      val affected = removedCaptions(current, document(reverted))
      all {
        current.clips == cut.newClips,
        current.bossRange == cut.newBossRange,
        if (affected != Set()) record(state.session, Impact, Idle,
          Awaiting({ target: target, removedCaptions: affected }), NoOutcome, state.archive)
        else record(reverted, Reverted, Idle, NoDecision, Applied, state.archive),
      }
    },
  }

  action decideRevert(accept) = match state.decision {
    | Awaiting(notice) => if (accept)
        record(reversion(state.session, notice.target), Confirmed, Idle, NoDecision, Applied, state.archive)
      else record(state.session, Declined, Idle, NoDecision, Discarded, state.archive)
    | NoDecision => all { false, state' = state }
  }

  action submitAny = {
    nondet prompt = Set(BossCut, Highlights, TrimWaiting, ReorderAndRaise, YellowText,
      ZoomScene, InsertSound, Portrait, CutAndGain, AdjustBoss).oneOf()
    submit(prompt)
  }

  action askAnyRevert = {
    nondet target = Set(0, 1, 2, 3).oneOf()
    askRevert(target)
  }

  // The finite exploration permits arbitrary operations within a bounded trace.
  action step = if (state.ticks >= 7) state' = state else any {
    submitAny,
    reply(Normal),
    reply(Raised),
    reply(InvalidGain),
    cancel,
    manualCaption(Boss),
    manualCaption(Highlight),
    moveHistory(Gui, true),
    moveHistory(Gui, false),
    moveHistory(Chat, true),
    moveHistory(Chat, false),
    save,
    reopen,
    askAnyRevert,
    decideRevert(true),
    decideRevert(false),
  }

  val workflowRules = {
    val session = state.session
    val before = state.previous
    val current = document(session)
    val prior = document(before)
    val historyOffset = match state.event {
      | Undo(_) => -1
      | Redo(_) => 1
      | _ => 0
    }
    val reverted = state.event == Reverted or state.event == Confirmed
    all {
      session.cursor >= 0 and session.cursor < session.versions.length(),
      session.versions.length() <= 5 and session.audit.length() <= 4,
      state.archive.length() <= 1,
      current.gain != InvalidGain,
      Set(originalBossRange, adjustedBossRange).contains(current.bossRange),
      hasScene(source, Boss) and current.clips.foldl(true, (valid, clip) => valid and hasScene(source, clip)),
      (state.event == Reply and state.outcome == InvalidVolume) implies all {
        session.versions == before.versions and session.cursor == before.cursor,
        session.audit == before.audit,
        session.conversation.length() == before.conversation.length() + 1,
        lastEntry(session.conversation).outcome == InvalidVolume,
      },
      (state.event == Reply and state.outcome == Discarded) implies (session == before),
      (state.event == Impact or state.event == Declined) implies (session == before),
      (state.event == Impact) implies (match state.decision {
        | Awaiting(notice) => current.bossCaption and notice.removedCaptions == Set(Boss)
          and not(hasScene(session.audit[notice.target].oldClips, Boss))
        | NoDecision => false
      }),
      (historyOffset != 0) implies all {
        session.audit == before.audit and session.versions == before.versions,
        session.cursor == before.cursor + historyOffset,
      },
      (state.event == Saved) implies (state.archive == [session] and session == before),
      (state.event != Saved) implies state.archive == state.priorArchive,
      (state.event == Reopened) implies (state.archive == [session]),
      (state.event != Reopened) implies
        if (before.audit.length() == 0) true
        else if (session.audit.length() < before.audit.length()) false
        else session.audit.slice(0, before.audit.length()) == before.audit,
      reverted implies all {
        session.audit.length() == before.audit.length() + 1,
        lastEntry(session.audit).change == SelectiveRevert,
        lastEntry(session.audit).target >= 0 and lastEntry(session.audit).target < before.audit.length(),
        current.clips == before.audit[lastEntry(session.audit).target].oldClips,
        current.bossRange == before.audit[lastEntry(session.audit).target].oldBossRange,
        current.bossCaption == (prior.bossCaption and hasScene(current.clips, Boss)),
        current.highlightCaption == prior.highlightCaption and current.gain == prior.gain,
      },
    }
  }

  action seedHighlights = submit(Highlights).then(reply(Normal))
  action seedBoss = submit(BossCut).then(reply(Normal))
  val view = preview(document(state.session))
  val canUndo = undoAvailable(state.session)
  val canRedo = redoAvailable(state.session)

  // BDD: chat-editing.feature:4 [13]
  run bossCutTest = init.then(seedBoss)
    .expect(view.clips == [Boss])
    .then(manualCaption(Boss))
    .expect(view.clips == [Boss] and view.correctedBoss)

  // BDD: chat-editing.feature:4 [14]
  run highlightsTest = init.then(seedHighlights)
    .expect(view.clips == [Boss, Highlight])
    .then(manualCaption(Highlight))
    .expect(view.clips == [Boss, Highlight] and view.correctedHighlight)

  // BDD: chat-editing.feature:4 [15]
  run trimWaitingTest = init.then(submit(TrimWaiting)).then(reply(Normal))
    .expect(view.clips == [Boss, Highlight] and view.visibleCaptions == [Boss, Highlight])
    .then(manualCaption(Boss))
    .expect(view.correctedBoss and view.visibleCaptions == [Boss, Highlight])

  // BDD: chat-editing.feature:17
  run continueChatTest = init.then(seedHighlights).then(submit(ReorderAndRaise)).then(reply(Normal))
    .expect(view.clips == [Highlight, Boss] and view.gain == Raised
      and listMap(state.session.conversation, turn => turn.prompt) == [Highlights, ReorderAndRaise])
    .then(manualCaption(Highlight))
    .expect(view.clips == [Highlight, Boss] and view.gain == Raised and view.correctedHighlight)

  // BDD: chat-editing.feature:24 [31]
  run colorTextTest = init.then(submit(TrimWaiting)).then(reply(Normal))
    .then(submit(YellowText)).then(reply(Normal))
    .expect(view.textColor == "yellow" and view.visibleCaptions == [Boss, Highlight] and view.gain == Normal)

  // BDD: chat-editing.feature:24 [32]
  run zoomTest = init.then(submit(TrimWaiting)).then(reply(Normal))
    .then(submit(ZoomScene)).then(reply(Normal))
    .expect(view.zoomAt == [Boss] and view.clips == [Boss, Highlight])

  // BDD: chat-editing.feature:24 [33]
  run insertSoundTest = init.then(submit(TrimWaiting)).then(reply(Normal))
    .then(submit(InsertSound)).then(reply(Normal))
    .expect(view.soundAt == 1 and view.clips == [Boss, Highlight])

  // BDD: chat-editing.feature:24 [34]
  run portraitTest = init.then(submit(TrimWaiting)).then(reply(Normal))
    .then(submit(Portrait)).then(reply(Normal))
    .expect(view.aspect == "9:16" and view.clips == [Boss, Highlight])

  // BDD: chat-editing.feature:36
  run lateReplyTest = init.then(submit(Highlights)).then(cancel).then(manualCaption(Highlight))
    .then(reply(Normal))
    .expect(view.clips == [] and view.correctedHighlight and state.outcome == Discarded)

  // BDD: chat-editing.feature:43
  run atomicFailureTest = init.then(seedHighlights).then(submit(CutAndGain)).then(reply(InvalidGain))
    .expect(view.clips == [Boss, Highlight] and view.gain == Normal
      and state.session.audit.length() == 1
      and lastEntry(state.session.conversation).outcome == InvalidVolume)

  // BDD: edit-history.feature:5
  run restoreConversationTest = init.then(seedHighlights).then(manualCaption(Highlight))
    .then(save).then(manualCaption(Highlight)).then(reopen)
    .expect(view.correctedHighlight and listMap(state.session.audit, change => change.author) == [Ai, Human]
      and listMap(state.session.audit, change => change.change) == [Cut, Caption]
      and state.session.conversation == [{
        prompt: Highlights, outcome: Applied, selected: [Boss, Highlight],
      }])
    .then(submit(ReorderAndRaise)).then(reply(Normal))
    .expect(listMap(state.session.conversation, turn => turn.prompt) == [Highlights, ReorderAndRaise]
      and view.clips == [Highlight, Boss] and view.gain == Raised and view.correctedHighlight)

  action seedCompound = submit(CutAndGain).then(reply(Raised))

  // BDD: edit-history.feature:13 [22]
  run guiBatchHistoryTest = init.then(seedCompound)
    .expect(view.clips == [Boss] and view.gain == Raised and state.session.audit.length() == 1)
    .then(moveHistory(Gui, true)).expect(view.clips == [] and view.gain == Normal)
    .then(moveHistory(Gui, false)).expect(view.clips == [Boss] and view.gain == Raised)

  // BDD: edit-history.feature:13 [23]
  run chatBatchHistoryTest = init.then(seedCompound)
    .then(moveHistory(Chat, true)).expect(view.clips == [] and view.gain == Normal)
    .then(moveHistory(Chat, false)).expect(view.clips == [Boss] and view.gain == Raised)

  // BDD: edit-history.feature:26
  run preserveIndependentCaptionTest = init.then(seedHighlights).then(submit(AdjustBoss)).then(reply(Normal))
    .then(manualCaption(Highlight))
    .expect(view.clips == [Boss, Highlight] and view.bossRange == adjustedBossRange and view.correctedHighlight)
    .then(askRevert(1))
    .expect(view.clips == [Boss, Highlight] and view.bossRange == originalBossRange and view.correctedHighlight
      and lastEntry(state.session.audit).change == SelectiveRevert and lastEntry(state.session.audit).target == 1)

  action affectedRevert = seedBoss.then(manualCaption(Boss)).then(askRevert(0))

  // BDD: edit-history.feature:34 [44]
  run confirmDependentRevertTest = init.then(affectedRevert)
    .expect(state.decision == Awaiting({ target: 0, removedCaptions: Set(Boss) })
      and view.clips == [Boss] and view.bossCaptionText == "boss-battle")
    .then(decideRevert(true))
    .expect(state.decision == NoDecision and view.clips == [] and view.bossCaptionText == ""
      and lastEntry(state.session.audit).change == SelectiveRevert and lastEntry(state.session.audit).target == 0)

  // BDD: edit-history.feature:34 [45]
  run declineDependentRevertTest = init.then(affectedRevert)
    .expect(state.decision == Awaiting({ target: 0, removedCaptions: Set(Boss) })
      and view.clips == [Boss] and view.bossCaptionText == "boss-battle")
    .then(decideRevert(false))
    .expect(state.decision == NoDecision and view.clips == [Boss] and view.bossCaptionText == "boss-battle"
      and state.session.audit.length() == 2)

  action undoCut = seedBoss.then(moveHistory(Gui, true))

  // BDD: edit-history.feature:47
  run clearRedoTest = init.then(undoCut).expect(canRedo).then(manualCaption(Highlight))
    .expect(view.correctedHighlight and not(canRedo))

  // BDD: edit-history.feature:54
  run retainDiscardedCutTest = init.then(undoCut).expect(canRedo).then(manualCaption(Highlight))
    .expect(not(canRedo) and listMap(state.session.audit, change => change.change) == [Cut, Caption]
      and state.session.audit[0].author == Ai)

  // BDD: edit-history.feature:59
  run reopenAndRedoTest = init.then(seedBoss).then(manualCaption(Boss))
    .then(submit(AdjustBoss)).then(reply(Normal))
    .expect(view.clips == [Boss] and view.bossCaptionText == "boss-battle" and view.bossRange == adjustedBossRange)
    .then(moveHistory(Gui, true))
    .expect(view.clips == [Boss] and view.bossCaptionText == "boss-battle" and view.bossRange == originalBossRange)
    .then(save).then(manualCaption(Boss))
    .expect(view.bossCaptionText == "" and not(canRedo))
    .then(reopen)
    .expect(view.clips == [Boss] and view.bossCaptionText == "boss-battle"
      and view.bossRange == originalBossRange and canUndo and canRedo
      and state.archive == state.priorArchive)
    .then(moveHistory(Gui, false))
    .expect(view.clips == [Boss] and view.bossCaptionText == "boss-battle" and view.bossRange == adjustedBossRange)
}
