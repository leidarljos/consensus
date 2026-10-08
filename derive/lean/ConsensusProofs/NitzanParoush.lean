import Mathlib

/-!
# Log-odds weights decide as the evidence does

Two options, A and B, at even prior odds. Voter `i` is right with
probability `p i ∈ (0, 1)`, independently of the others, and `v i` says
whether it voted A. The likelihood of the ballots under A is
`∏ i, (if v i then p i else 1 - p i)`, under B the same with the roles swapped.

`log_likelihood_ratio`: the log of their ratio is the weighted vote
`∑ i, ± logit (p i)`, plus for A and minus for B.

`map_iff_weighted_vote`: so A is at least as likely as B exactly when that
weighted vote is nonnegative. A weighted majority with weights
`log (p / (1 - p))` is the most probable answer, Nitzan and Paroush's theorem
(doi:10.2307/2526438), and the reason `calibrate` writes log-odds rows.
-/

open Finset

namespace ConsensusProofs

variable {n : ℕ}

/-- The log odds of being right. -/
noncomputable def logit (p : ℝ) : ℝ := Real.log (p / (1 - p))

/-- The likelihood of the ballots `v` when A is the answer. -/
noncomputable def likA (p : Fin n → ℝ) (v : Fin n → Bool) : ℝ :=
  ∏ i, if v i then p i else 1 - p i

/-- The likelihood of the ballots `v` when B is the answer. -/
noncomputable def likB (p : Fin n → ℝ) (v : Fin n → Bool) : ℝ :=
  ∏ i, if v i then 1 - p i else p i

/-- Voter `i`'s signed weight: its log odds, for A or against. -/
noncomputable def vote (p : Fin n → ℝ) (v : Fin n → Bool) (i : Fin n) : ℝ :=
  if v i then logit (p i) else -logit (p i)

theorem likA_pos {p : Fin n → ℝ} (hp : ∀ i, 0 < p i ∧ p i < 1) (v : Fin n → Bool) :
    0 < likA p v :=
  Finset.prod_pos fun i _ => by
    split_ifs
    · exact (hp i).1
    · exact sub_pos.2 (hp i).2

theorem likB_pos {p : Fin n → ℝ} (hp : ∀ i, 0 < p i ∧ p i < 1) (v : Fin n → Bool) :
    0 < likB p v :=
  Finset.prod_pos fun i _ => by
    split_ifs
    · exact sub_pos.2 (hp i).2
    · exact (hp i).1

/-- The log likelihood ratio of A over B is the log-odds weighted vote. -/
theorem log_likelihood_ratio {p : Fin n → ℝ} (hp : ∀ i, 0 < p i ∧ p i < 1)
    (v : Fin n → Bool) :
    Real.log (likA p v / likB p v) = ∑ i, vote p v i := by
  have hA : ∀ i ∈ (univ : Finset (Fin n)), (if v i then p i else 1 - p i) ≠ 0 := by
    intro i _
    split_ifs
    · exact (hp i).1.ne'
    · exact (sub_pos.2 (hp i).2).ne'
  have hB : ∀ i ∈ (univ : Finset (Fin n)), (if v i then 1 - p i else p i) ≠ 0 := by
    intro i _
    split_ifs
    · exact (sub_pos.2 (hp i).2).ne'
    · exact (hp i).1.ne'
  rw [Real.log_div (likA_pos hp v).ne' (likB_pos hp v).ne', likA, likB,
    Real.log_prod hA, Real.log_prod hB, ← Finset.sum_sub_distrib]
  refine Finset.sum_congr rfl fun i _ => ?_
  have h0 : p i ≠ 0 := (hp i).1.ne'
  have h1 : 1 - p i ≠ 0 := (sub_pos.2 (hp i).2).ne'
  unfold vote logit
  split_ifs
  · rw [Real.log_div h0 h1]
  · rw [Real.log_div h0 h1]; ring

/-- At even prior odds A is at least as probable as B exactly when the
log-odds weighted vote for A is nonnegative. -/
theorem map_iff_weighted_vote {p : Fin n → ℝ} (hp : ∀ i, 0 < p i ∧ p i < 1)
    (v : Fin n → Bool) :
    likB p v ≤ likA p v ↔ 0 ≤ ∑ i, vote p v i := by
  rw [← log_likelihood_ratio hp v,
    Real.log_nonneg_iff (div_pos (likA_pos hp v) (likB_pos hp v)),
    one_le_div (likB_pos hp v)]

end ConsensusProofs
