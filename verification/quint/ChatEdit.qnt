module ChatEdit {
  type Reply = Waiting | SelectedClip | SelectedWholeSource | Failed
  type Request = Idle | Pending | Cancelled | Invalidated
  type Clip = NoClip | ManualClip | AiClip
  type ClipHistory = NoClipHistory | ManualClipHistory | AiClipHistory
  type Edit = {
    source: bool,
    transcript: bool,
    caption: bool,
    clip: Clip,
    clipHistory: ClipHistory,
  }
  type State = {
    request: Request,
    reply: Reply,
    draft: str,
    submittedDraft: str,
    edit: Edit,
    beforePoll: Edit,
    mustPreserveEdits: bool,
  }

  var state: State

  pure val initialEdit = {
    source: false,
    transcript: false,
    caption: false,
    clip: NoClip,
    clipHistory: NoClipHistory,
  }

  action init = state' = {
    request: Idle,
    reply: Waiting,
    draft: "original",
    submittedDraft: "original",
    edit: initialEdit,
    beforePoll: initialEdit,
    mustPreserveEdits: false,
  }

  action submit = all {
    state.request == Idle,
    state.edit.clip == NoClip,
    state' = { ...state, request: Pending, submittedDraft: state.draft },
  }

  action cancel = all {
    state.request == Pending,
    state' = { ...state, request: Cancelled },
  }

  // These committed changes invalidate the request even if the input later changes back.
  action changeEdit(edit) = state' = {
    ...state,
    request: if (state.request == Pending) Invalidated else state.request,
    edit: edit,
    mustPreserveEdits: false,
  }

  action editCaption = changeEdit({ ...state.edit, caption: not(state.edit.caption) })

  action replaceSource = changeEdit({ ...initialEdit, source: not(state.edit.source) })

  action replaceTranscript = changeEdit({
    ...state.edit,
    transcript: not(state.edit.transcript),
    caption: false,
    clipHistory: NoClipHistory,
  })

  action addManualClip = all {
    state.edit.clip == NoClip,
    changeEdit({ ...state.edit, clip: ManualClip, clipHistory: ManualClipHistory }),
  }

  action editDraft(draft) = state' = { ...state, draft: draft }

  action arrive(reply) = all {
    state.request != Idle,
    state.reply == Waiting,
    state' = { ...state, reply: reply },
  }

  val canApply =
    state.request == Pending and state.reply == SelectedClip and state.draft == state.submittedDraft

  action poll = all {
    state.reply != Waiting,
    state' = {
      ...state,
      request: Idle,
      reply: Waiting,
      beforePoll: state.edit,
      mustPreserveEdits: state.request == Cancelled or state.request == Invalidated
        or state.reply == Failed or state.reply == SelectedWholeSource
        or state.draft != state.submittedDraft,
      edit: if (canApply) { ...state.edit, clip: AiClip, clipHistory: AiClipHistory } else state.edit,
    },
  }

  action step = any {
    submit,
    cancel,
    editCaption,
    replaceSource,
    replaceTranscript,
    addManualClip,
    editDraft("original"),
    editDraft("manual"),
    arrive(SelectedClip),
    arrive(SelectedWholeSource),
    arrive(Failed),
    poll,
  }

  val pollPreservesEditsWhenRequired = state.mustPreserveEdits implies state.edit == state.beforePoll

  run cancelledReplyTest = init.then(submit).then(cancel).then(editCaption)
    .then(arrive(SelectedClip)).then(poll)
    .expect(state.edit == { ...initialEdit, caption: true })

  run cancelledAfterArrivalTest = init.then(submit).then(arrive(SelectedClip))
    .then(cancel).then(poll)
    .expect(state.edit == initialEdit)

  run captionChangedBeforePollTest = init.then(submit).then(arrive(SelectedClip))
    .then(editCaption).then(poll)
    .expect(state.edit == { ...initialEdit, caption: true })

  run staleDraftTest = init.then(submit).then(arrive(SelectedClip))
    .then(editDraft("manual")).then(poll)
    .expect(state.edit == initialEdit)

  run failedReplyTest = init.then(submit).then(arrive(Failed)).then(poll)
    .expect(state.edit == initialEdit)

  run freshReplyTest = init.then(submit).then(arrive(SelectedClip)).then(poll)
    .expect(state.edit == { ...initialEdit, clip: AiClip, clipHistory: AiClipHistory })

  run freshReplyForChangedDraftTest = init.then(editDraft("manual"))
    .then(submit).then(arrive(SelectedClip)).then(poll)
    .expect(state.edit == { ...initialEdit, clip: AiClip, clipHistory: AiClipHistory })

  run wholeSourceReplyTest = init.then(submit).then(arrive(SelectedWholeSource)).then(poll)
    .expect(state.edit == initialEdit)
    .then(submit).expect(state.request == Pending)

  run replacedSourceTest = init.then(submit).then(replaceSource)
    .then(arrive(SelectedClip)).then(poll)
    .expect(state.edit == { ...initialEdit, source: true })

  run replacedTranscriptTest = init.then(submit).then(replaceTranscript)
    .then(arrive(SelectedClip)).then(poll)
    .expect(state.edit == { ...initialEdit, transcript: true })

  run manualClipBeforePollTest = init.then(submit).then(arrive(SelectedClip))
    .then(addManualClip).then(poll)
    .expect(state.edit == { ...initialEdit, clip: ManualClip, clipHistory: ManualClipHistory })

  run transcriptReplacementKeepsClipAndClearsHistoryTest = init.then(submit)
    .then(addManualClip).then(replaceTranscript).then(arrive(SelectedClip)).then(poll)
    .expect(state.edit == { ...initialEdit, transcript: true, clip: ManualClip })

  run captionChangedBackTest = init.then(submit).then(editCaption).then(editCaption)
    .then(arrive(SelectedClip)).then(poll)
    .expect(state.edit == initialEdit)
}
