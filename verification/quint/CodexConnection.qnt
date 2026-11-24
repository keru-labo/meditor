module CodexConnection {
  type Execution = { job: str, host: str }
  type State = {
    phase: str,
    status: str,
    loginGuidance: str,
    authenticated: bool,
    codeMode: bool,
    modelMatches: bool,
    payload: Set[str],
    mediaReady: bool,
    executions: Set[Execution],
    editRevision: int,
    beforeEdit: int,
    beforePayload: Set[str],
    last: str,
  }

  var state: State

  pure val projectFields = Set("prompt", "timed-captions", "source-bounds", "video", "audio", "local-path")
  pure val textFields = Set("prompt", "timed-captions", "source-bounds")
  pure val localJobs = Set("transcription", "speaker-identification", "audio-separation")

  action init = state' = {
    phase: "idle",
    status: "idle",
    loginGuidance: "",
    authenticated: false,
    codeMode: false,
    modelMatches: false,
    payload: Set(),
    mediaReady: false,
    executions: Set(),
    editRevision: 0,
    beforeEdit: 0,
    beforePayload: Set(),
    last: "init",
  }

  action update(revision, event) = state' = {
    ...revision,
    beforeEdit: state.editRevision,
    beforePayload: state.payload,
    last: event,
  }

  action requestSend = all {
    state.phase == "idle",
    update({ ...state, phase: "checking", status: "checking" }, "request"),
  }

  // Authentication is the account reported for the editor's dedicated Codex profile.
  action report(authenticated, codeMode, modelMatches) = all {
    state.phase == "checking",
    val status =
      if (codeMode) "settings-unconfirmed"
      else if (not(authenticated)) "login-required"
      else if (not(modelMatches)) "settings-unconfirmed"
      else "ready"
    update({
      ...state, phase: if (status == "ready") "ready" else "rejected",
      authenticated: authenticated, codeMode: codeMode, modelMatches: modelMatches, status: status,
      loginGuidance: if (status == "login-required") "codex login" else "",
    }, "report"),
  }

  action sendText = all {
    state.phase == "ready",
    update({
      ...state, phase: "sent", status: "sent", payload: projectFields.intersect(textFields),
    }, "send"),
  }

  action loadLocalMedia = update({ ...state, mediaReady: true, executions: Set() }, "load")

  // Speaker identification and audio separation specify the local boundary for planned implementations.
  action runLocally(job) = all {
    state.mediaReady and localJobs.contains(job),
    update({
      ...state, executions: state.executions.union(Set({ job: job, host: "windows-pc" })),
    }, "local"),
  }

  action reviseEdit = update({ ...state, editRevision: 1 - state.editRevision }, "revise")

  action step = any {
    requestSend,
    report(true, false, true),
    report(false, false, true),
    report(true, true, true),
    report(true, false, false),
    sendText,
    loadLocalMedia,
    runLocally("transcription"),
    runLocally("speaker-identification"),
    runLocally("audio-separation"),
    reviseEdit,
  }

  val workflowRules = all {
    state.payload.subseteq(textFields),
    (state.phase == "sent") implies all {
      state.authenticated and not(state.codeMode) and state.modelMatches,
      state.payload == textFields,
    },
    (state.phase != "sent") implies state.payload == Set(),
    (state.status == "login-required") implies (state.loginGuidance == "codex login"),
    (state.status != "login-required") implies (state.loginGuidance == ""),
    (state.last != "revise") implies state.editRevision == state.beforeEdit,
    state.executions.forall(execution =>
      execution.host == "windows-pc" and localJobs.contains(execution.job)
    ),
    (state.executions != Set()) implies state.mediaReady,
    (state.last == "local") implies state.payload == state.beforePayload,
  }

  // BDD: ai-connections.feature:4
  run authenticatedSendTest = init.then(requestSend).then(report(true, false, true))
    .expect(state.phase == "ready" and state.payload == Set())
    .then(sendText)
    .expect(state.payload.contains("prompt") and state.payload.contains("timed-captions"))
    .expect(state.payload.intersect(Set("video", "audio", "local-path")) == Set())

  // BDD: ai-connections.feature:11
  run loginRequiredTest = init.then(reviseEdit).then(requestSend).then(report(false, false, true))
    .expect(state.status == "login-required" and state.phase == "rejected")
    .expect(state.loginGuidance == "codex login" and state.payload == Set() and state.editRevision == 1)

  // BDD: ai-connections.feature:18
  run codeModeRejectedTest = init.then(reviseEdit).then(requestSend).then(report(true, true, true))
    .expect(state.status == "settings-unconfirmed" and state.phase == "rejected")
    .expect(state.payload == Set() and state.editRevision == 1)

  // BDD: ai-connections.feature:18
  run wrongModelRejectedTest = init.then(reviseEdit).then(requestSend).then(report(true, false, false))
    .expect(state.status == "settings-unconfirmed" and state.phase == "rejected")
    .expect(state.payload == Set() and state.editRevision == 1)

  // BDD: ai-connections.feature:26
  run mediaProcessingRemainsLocalTest = init.then(loadLocalMedia).then(runLocally("transcription"))
    .expect(state.executions == Set({ job: "transcription", host: "windows-pc" }) and state.payload == Set())
    .then(runLocally("speaker-identification"))
    .expect(state.executions.contains({ job: "speaker-identification", host: "windows-pc" }))
    .expect(state.payload == Set())
    .then(runLocally("audio-separation"))
    .expect(state.executions.contains({ job: "audio-separation", host: "windows-pc" }))
    .expect(state.executions.size() == 3 and state.payload == Set())
}
