module EditHistory {
  type Snapshot = { caption: int, clip: int }
  type Timeline = { snapshots: List[Snapshot], cursor: int }
  type Command = { kind: str, value: int }
  type State = {
    before: Timeline,
    after: Timeline,
    savedBefore: List[Timeline],
    saved: List[Timeline],
    command: Command,
    result: str,
  }

  var state: State

  pure val initial = { snapshots: [{ caption: 0, clip: 0 }], cursor: 0 }
  // Bound the number of snapshots for exhaustive exploration.
  pure val maxSnapshots = 4

  action init = state' = {
    before: initial,
    after: initial,
    savedBefore: [],
    saved: [],
    command: { kind: "init", value: 0 },
    result: "unchanged",
  }

  pure def current(timeline) = timeline.snapshots[timeline.cursor]

  action recordWithSave(kind, value, after, result, saved) = state' = {
    before: state.after,
    after: after,
    savedBefore: state.saved,
    saved: saved,
    command: { kind: kind, value: value },
    result: result,
  }

  action record(kind, value, after, result) =
    recordWithSave(kind, value, after, result, state.saved)

  action edit(kind, value, snapshot) = {
    val timeline = state.after
    if (snapshot == current(timeline)) {
      record(kind, value, timeline, "unchanged")
    } else {
      val destination = timeline.cursor + 1
      all {
        destination < maxSnapshots,
        record(kind, value, {
          snapshots: timeline.snapshots.slice(0, destination).append(snapshot),
          cursor: destination,
        }, "changed"),
      }
    }
  }

  action caption(value) = edit("caption", value, { ...current(state.after), caption: value })

  action clip(value) =
    if (value < 0) record("clip", value, state.after, "error")
    else edit("clip", value, { ...current(state.after), clip: value })

  action move(kind, offset) = {
    val timeline = state.after
    val destination = timeline.cursor + offset
    if (destination < 0 or destination >= timeline.snapshots.length()) {
      record(kind, 0, timeline, "unchanged")
    } else {
      record(kind, 0, { ...timeline, cursor: destination }, "changed")
    }
  }

  action step = any {
    caption(0),
    caption(1),
    clip(0),
    clip(1),
    clip(-1),
    move("undo", -1),
    move("redo", 1),
  }

  // One save slot for a complete archive within the persistence limits.
  action save = recordWithSave("save", 0, state.after, "unchanged", [state.after])

  action reopen = all {
    state.saved.length() == 1,
    record("reopen", 0, state.saved[0], "unchanged"),
  }

  action persistentStep = any { step, save, reopen }

  // The unsaved caption edit discards Redo. Reopening must recover it from disk.
  action checkpointStep = {
    val kind = state.command.kind
    any {
      all { kind == "init", caption(1) },
      all { kind == "caption", state.command.value == 1, clip(1) },
      all { kind == "clip", move("undo", -1) },
      all { kind == "undo", state.saved.length() == 0, save },
      all { kind == "save", caption(0) },
      all { kind == "caption", state.command.value == 0, reopen },
      all { kind == "reopen", move("redo", 1) },
      all { kind == "redo", move("undo", -1) },
      all { kind == "undo", state.saved.length() == 1, state.after.cursor == 1,
        move("undo", -1) },
    }
  }

  val historyRules = {
    val before = state.before
    val after = state.after
    val kind = state.command.kind
    val value = state.command.value
    val was = current(before)
    val now = current(after)
    val isEdit = kind == "caption" or kind == "clip"
    val invalid = kind == "clip" and value < 0
    val sameValue = (kind == "caption" and value == was.caption)
      or (kind == "clip" and value == was.clip)
    val canUndo = before.cursor > 0
    val canRedo = before.cursor + 1 < before.snapshots.length()
    all {
      after.cursor >= 0,
      after.cursor < after.snapshots.length(),
      after.snapshots.length() <= maxSnapshots,
      state.saved.length() <= 1,
      (kind == "save") implies all {
        after == before,
        state.saved == [before],
        state.result == "unchanged",
      },
      (kind != "save") implies state.saved == state.savedBefore,
      (kind == "reopen") implies all {
        state.savedBefore == [after],
        state.result == "unchanged",
      },
      invalid implies (after == before and state.result == "error"),
      (isEdit and sameValue) implies (after == before and state.result == "unchanged"),
      (isEdit and not(invalid or sameValue)) implies all {
        state.result == "changed",
        after.cursor == before.cursor + 1,
        after.snapshots.length() == after.cursor + 1,
        after.snapshots.slice(0, after.cursor) == before.snapshots.slice(0, before.cursor + 1),
        (kind == "caption") implies (now.caption == value and now.clip == was.clip),
        (kind == "clip") implies (now.clip == value and now.caption == was.caption),
      },
      (kind == "undo" and canUndo) implies all {
        state.result == "changed",
        after.snapshots == before.snapshots,
        after.cursor + 1 == before.cursor,
      },
      (kind == "redo" and canRedo) implies all {
        state.result == "changed",
        after.snapshots == before.snapshots,
        after.cursor == before.cursor + 1,
      },
      ((kind == "undo" and not(canUndo)) or (kind == "redo" and not(canRedo)))
        implies (after == before and state.result == "unchanged"),
    }
  }

  run saveReopenRedoTest = init.then(caption(1)).then(clip(1)).then(move("undo", -1))
    .then(save).then(reopen)
    .expect(current(state.after) == { caption: 1, clip: 0 }
      and state.after.cursor == 1 and state.after.snapshots.length() == 3)
    .then(move("redo", 1))
    .expect(current(state.after) == { caption: 1, clip: 1 }
      and state.after.cursor == 2)
    .then(move("undo", -1)).then(move("undo", -1))
    .expect(current(state.after) == { caption: 0, clip: 0 } and state.after.cursor == 0)
}
