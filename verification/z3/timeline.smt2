; Integer model of meditor's clip and playhead arithmetic.
(set-logic QF_NIA)
(set-option :timeout 5000)

(define-fun max-duration () Int (* 24 60 60 1000000))
(define-fun slider-range () Int 1000000)
(define-fun i64-max () Int 9223372036854775807)
(define-fun i32-max () Int 2147483647)

(define-fun clamp ((x Int) (lo Int) (hi Int)) Int
  (ite (< x lo) lo (ite (> x hi) hi x)))

(declare-const clip-start Int)
(declare-const clip-end Int)
(declare-const slider Int)
(declare-const position Int)
(declare-const new-start Int)
(declare-const new-end Int)
(declare-const decoder-duration Int)

; ClipRange::valid_for and the editor's clamped position.
(assert (and (<= 0 clip-start) (< clip-start clip-end) (<= clip-end max-duration)))
(assert (and (<= 0 new-start) (< new-start new-end) (<= new-end max-duration)))
(define-fun duration () Int (- clip-end clip-start))
(assert (and (<= 0 position) (<= position duration)))
(assert (and (<= 0 slider) (<= slider slider-range)))
(assert (and (< 0 decoder-duration) (<= decoder-duration max-duration)))

; Feasibility check. Contradictory assumptions would invalidate every result below.
(echo "valid inputs")
(check-sat)

(define-fun slider-numerator () Int (* duration slider))
(define-fun slider-position () Int (div slider-numerator slider-range))
(define-fun source-target () Int (+ clip-start slider-position))

(echo "slider position and source target stay in range")
(push)
(assert (not (and (<= 0 slider-numerator) (<= slider-numerator i64-max)
                  (<= 0 slider-position) (<= slider-position duration)
                  (<= clip-start source-target) (<= source-target clip-end))))
(check-sat)
(pop)

(echo "only the final slider value reaches the clip end")
(push)
(assert (not (= (= slider slider-range) (= source-target clip-end))))
(check-sat)
(pop)

; For positive divisors the SMT quotient of a nonnegative numerator matches Rust.
(define-fun display-numerator () Int (* position slider-range))
(define-fun playhead () Int (div display-numerator duration))

(echo "playhead fits the slider and its i32 cast")
(push)
(assert (not (and (<= 0 display-numerator) (<= display-numerator i64-max)
                  (<= 0 playhead) (<= playhead slider-range)
                  (<= playhead i32-max))))
(check-sat)
(pop)

(define-fun old-source-position () Int (+ clip-start position))
(define-fun requested-position () Int (- old-source-position new-start))
(define-fun new-position () Int (clamp requested-position 0 (- new-end new-start)))

(echo "clip edits retain the source position or clamp it to the new clip")
(push)
(assert (not (= (+ new-start new-position)
                (clamp old-source-position new-start new-end))))
(check-sat)
(pop)

(define-fun decoder-end () Int
  (ite (< clip-end decoder-duration) clip-end decoder-duration))
(define-fun decoder-target () Int (clamp source-target 0 (- decoder-end 1)))

(echo "decoder clamps the target below the exclusive end")
(push)
(assert (not (and (<= 0 decoder-target) (< decoder-target decoder-end)
                  (<= decoder-end clip-end) (<= decoder-end decoder-duration))))
(check-sat)
(pop)
