import Mathlib

/-!
# Self-trust and the weighted vote

`learn` and `calibrate` write rows that weigh voter `j` alike from everyone:
`w j` in every other voter's row. What the settle does with them depends on
the diagonal.

`earned_idempotent` and `earned_one_round`: when each voter weighs itself as
the others weigh it, every row is `w / S` and the matrix is idempotent, and
one round leaves every voter on the weighted vote `∑ j, w j * x0 j / S`,
which stays fixed.

`constant_stationary`: with one self-weight `c` for every voter instead, row
`i` is `w` off the diagonal and `c` on it, divided by `S - w i + c`, and the
weight the DeGroot limit gives voter `j` is proportional to
`w j * (S + c - w j)`, not `w j`. derive/sympy/fj.py checks the same two
identities symbolically; examples/synthetic_voters.rs measures what the
difference costs a group.
-/

open Finset

namespace ConsensusProofs

variable {n : ℕ}

/-- Every row is the weights over their sum. -/
noncomputable def earnedRows (w : Fin n → ℝ) : Matrix (Fin n) (Fin n) ℝ :=
  Matrix.of fun _ j => w j / ∑ k, w k

/-- Earned self-trust makes the trust matrix idempotent: one round settles. -/
theorem earned_idempotent (w : Fin n → ℝ) (hS : ∑ k, w k ≠ 0) :
    earnedRows w * earnedRows w = earnedRows w := by
  ext i j
  simp only [earnedRows, Matrix.mul_apply, Matrix.of_apply]
  rw [← Finset.sum_mul, ← Finset.sum_div, div_self hS, one_mul]

/-- One round leaves every voter on the weighted vote. -/
theorem earned_one_round (w x0 : Fin n → ℝ) (i : Fin n) :
    (earnedRows w).mulVec x0 i = (∑ j, w j * x0 j) / ∑ k, w k := by
  simp only [earnedRows, Matrix.mulVec, dotProduct, Matrix.of_apply]
  rw [Finset.sum_div]
  refine Finset.sum_congr rfl fun j _ => ?_
  ring

/-- Rows with one self-weight `c` for every voter. -/
noncomputable def constantRows (w : Fin n → ℝ) (c : ℝ) : Fin n → Fin n → ℝ :=
  fun i j => (if i = j then c else w j) / ((∑ k, w k) - w i + c)

/-- Under a constant self-weight the stationary weights are
`w j * (S + c - w j)`: `∑ i, π i * W i j = π j`. -/
theorem constant_stationary (w : Fin n → ℝ) (c : ℝ)
    (hden : ∀ i, (∑ k, w k) - w i + c ≠ 0) (j : Fin n) :
    ∑ i, w i * ((∑ k, w k) + c - w i) * constantRows w c i j
      = w j * ((∑ k, w k) + c - w j) := by
  set S := ∑ k, w k with hS
  have term : ∀ i, w i * (S + c - w i) * constantRows w c i j
      = w i * (if i = j then c else w j) := by
    intro i
    have hd := hden i
    simp only [constantRows]
    rw [← hS, show S + c - w i = S - w i + c by ring]
    field_simp
  rw [Finset.sum_congr rfl fun i _ => term i, ← Finset.add_sum_erase _ _ (Finset.mem_univ j)]
  have rest : ∑ i ∈ univ.erase j, w i * (if i = j then c else w j) = (S - w j) * w j := by
    have off : ∀ i ∈ univ.erase j, w i * (if i = j then c else w j) = w i * w j := by
      intro i hi
      simp [Finset.ne_of_mem_erase hi]
    rw [Finset.sum_congr rfl off, ← Finset.sum_mul, Finset.sum_erase_eq_sub (Finset.mem_univ j)]
  have head : w j * (if j = j then c else w j) = w j * c := by simp
  rw [rest, head]
  ring

end ConsensusProofs
