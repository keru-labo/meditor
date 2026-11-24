module ChatEdit {
  type Reply = Waiting | Valid | Failed
  type Request = Idle | Pending | Cancelled | Invalidated
  type Edit = { caption: str, aiClip: bool, aiHistory: bool }
  type State = {
    request: Request,
    reply: Reply,
    draft: str,
    submittedDraft: str,
    edit: Edit,
    beforePoll: Edit,
    mustDiscard: bool,
  }

  var state: State

  pure val initialEdit = { caption: "original", aiClip: false, aiHistory: false }

  action init = state' = {
    request: Idle,
    reply: Waiting,
    draft: "original",
    submittedDraft: "original",
    edit: initialEdit,
    beforePoll: initialEdit,
    mustDiscard: false,
  }

  action submit = all {
    state.request == Idle,
    not(state.edit.aiClip),
    state' = { ...state, request: Pending, submittedDraft: state.draft },
  }

  action cancel = all {
    state.request == Pending,
    state' = { ...state, request: Cancelled },
  }

  action editCaption = all {
    state.edit.caption == "original",
    state' = {
      ...state,
      request: if (state.request == Pending) Invalidated else state.request,
      edit: { ...state.edit, caption: "manual" },
      mustDiscard: false,
    },
  }

  action editDraft(draft) = state' = { ...state, draft: draft }

  action arrive(reply) = all {
    state.request != Idle,
    state.reply == Waiting,
    state' = { ...state, reply: reply },
  }

  val canApply =
    state.request == Pending and state.reply == Valid and state.draft == state.submittedDraft

  action poll = all {
    state.reply != Waiting,
    state' = {
      ...state,
      request: Idle,
      reply: Waiting,
      beforePoll: state.edit,
      mustDiscard: state.request == Cancelled or state.request == Invalidated
        or state.reply == Failed or state.draft != state.submittedDraft,
      edit: if (canApply) { ...state.edit, aiClip: true, aiHistory: true } else state.edit,
    },
  }

  action step = any {
    submit,
    cancel,
    editCaption,
    editDraft("original"),
    editDraft("manual"),
    arrive(Valid),
    arrive(Failed),
    poll,
  }

  val discardPreservesEdits = state.mustDiscard implies state.edit == state.beforePoll

  run cancelledReplyTest = init.then(submit).then(cancel).then(editCaption)
    .then(arrive(Valid)).then(poll)
    .expect(state.edit == { ...initialEdit, caption: "manual" })

  run cancelledAfterArrivalTest = init.then(submit).then(arrive(Valid))
    .then(cancel).then(poll)
    .expect(state.edit == initialEdit)

  run captionChangedBeforePollTest = init.then(submit).then(arrive(Valid))
    .then(editCaption).then(poll)
    .expect(state.edit == { ...initialEdit, caption: "manual" })

  run staleDraftTest = init.then(submit).then(arrive(Valid))
    .then(editDraft("manual")).then(poll)
    .expect(state.edit == initialEdit)

  run failedReplyTest = init.then(submit).then(arrive(Failed)).then(poll)
    .expect(state.edit == initialEdit)

  run freshReplyTest = init.then(submit).then(arrive(Valid)).then(poll)
    .expect(state.edit == { ...initialEdit, aiClip: true, aiHistory: true })
}
