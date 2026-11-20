namespace LatestSeek

abbrev State := Option Int

def bound (target duration : Int) : Int :=
  max 0 (min target (max 0 duration))

def request (_ : State) (target duration : Int) : State :=
  some (bound target duration)

def take (pending : State) : State × State :=
  (pending, none)

def clear (_ : State) : State :=
  none

theorem latest_wins (pending : State) (first last duration : Int) :
    take (request (request pending first duration) last duration) =
      (some (bound last duration), none) := rfl

theorem consume_once (pending : State) :
    take (take pending).2 = (none, none) := rfl

theorem cancel_pending (pending : State) :
    take (clear pending) = (none, none) := rfl

theorem target_within_duration (pending : State) (target duration : Int) :
    let position := (take (request pending target duration)).1.getD 0
    0 ≤ position ∧ position ≤ max 0 duration := by
  change 0 ≤ bound target duration ∧ bound target duration ≤ max 0 duration
  unfold bound
  omega

end LatestSeek
