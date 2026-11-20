import Lean

/- A model of the GUI-thread controller in rust/src/lib.rs. -/
namespace Meditor.SeekController

inductive Mode where
  | scrub (proxyAvailable : Bool)
  | settle (resumePlayback : Bool)
  deriving DecidableEq

structure Request where
  serial : Nat
  positionUs : Nat
  mode : Mode
  deriving DecidableEq

structure State where
  serial : Nat
  pending : Option Request

inductive Event where
  | request (positionUs : Nat) (mode : Mode)
  | invalidate
  | frame (result : Request)

def initial : State := ⟨0, none⟩

def issue (s : State) (positionUs : Nat) (mode : Mode) : State × Request :=
  let result : Request := ⟨s.serial + 1, positionUs, mode⟩
  (⟨result.serial, some result⟩, result)

def invalidate (s : State) : State := { s with pending := none }

def acceptFrame (s : State) (result : Request) : State × Bool :=
  if s.pending = some result then (invalidate s, true) else (s, false)

def step (s : State) : Event → State
  | .request positionUs mode => (issue s positionUs mode).1
  | .invalidate => invalidate s
  | .frame result => (acceptFrame s result).1

def run (s : State) : List Event → State
  | [] => s
  | event :: rest => run (step s event) rest

-- A pending request, when present, carries the latest issue serial.
def Valid (s : State) : Prop :=
  ∀ result, s.pending = some result → result.serial = s.serial

private theorem issue_valid (s : State) (positionUs : Nat) (mode : Mode) :
    Valid (issue s positionUs mode).1 := by
  simp [Valid, issue]

private theorem step_valid {s : State} (h : Valid s) (event : Event) :
    Valid (step s event) := by
  cases event with
  | request positionUs mode => exact issue_valid s positionUs mode
  | invalidate => simp [step, Valid, invalidate]
  | frame result =>
    by_cases hPending : s.pending = some result
    · simp [step, acceptFrame, hPending, Valid, invalidate]
    · simpa [step, acceptFrame, hPending] using h

private theorem run_valid {s : State} (h : Valid s) (events : List Event) :
    Valid (run s events) := by
  induction events generalizing s with
  | nil => exact h
  | cons event rest ih => exact ih (step_valid h event)

theorem reachable_pending_is_latest (events : List Event) : Valid (run initial events) :=
  run_valid (by simp [Valid, initial]) events

private theorem serial_step_le (s : State) (event : Event) : s.serial ≤ (step s event).serial := by
  cases event with
  | request => simp [step, issue]
  | invalidate => simp [step, invalidate]
  | frame result =>
    by_cases hPending : s.pending = some result <;>
      simp [step, acceptFrame, hPending, invalidate]

private theorem serial_run_le (s : State) (events : List Event) : s.serial ≤ (run s events).serial := by
  induction events generalizing s with
  | nil => exact Nat.le_refl _
  | cons event rest ih => exact Nat.le_trans (serial_step_le s event) (ih (step s event))

private theorem reject_older {s : State} (h : Valid s) {result : Request}
    (older : result.serial < s.serial) : (acceptFrame s result).2 = false := by
  have mismatch : s.pending ≠ some result := by
    intro equal
    exact (Nat.ne_of_lt older) (h result equal)
  simp [acceptFrame, mismatch]

/-- Once a newer request is issued, an older frame stays obsolete after any events. -/
theorem superseded_frame_stays_rejected (s : State) (positionUs nextPositionUs : Nat)
    (mode nextMode : Mode) (events : List Event) :
    let first := issue s positionUs mode
    let next := issue first.1 nextPositionUs nextMode
    (acceptFrame (run next.1 events) first.2).2 = false := by
  dsimp
  apply reject_older (run_valid (issue_valid _ _ _) events)
  have monotone := serial_run_le (issue (issue s positionUs mode).1 nextPositionUs nextMode).1 events
  change s.serial + 1 + 1 ≤ _ at monotone
  change s.serial + 1 < _
  omega

/-- Clearing the slot discards the completion from the media that was just closed. -/
theorem invalidated_frame_rejected (s : State) (result : Request) :
    (acceptFrame (invalidate s) result).2 = false := by
  simp [acceptFrame, invalidate]

/-- Presenting a frame consumes its request, so a duplicate cannot be presented. -/
theorem accepted_frame_consumed (s : State) (result : Request)
    (accepted : (acceptFrame s result).2 = true) :
    (acceptFrame (acceptFrame s result).1 result).2 = false := by
  by_cases hPending : s.pending = some result
  · simpa [acceptFrame, hPending] using invalidated_frame_rejected s result
  · simp [acceptFrame, hPending] at accepted

end Meditor.SeekController
