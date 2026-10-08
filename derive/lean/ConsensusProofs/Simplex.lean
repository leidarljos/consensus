import Mathlib

/-!
# Opinions stay distributions

A voter's opinion is a row over the options: nonnegative, summing to one. The
step `x(t+1) i k = s i * ∑ j, W i j * x(t) j k + (1 - s i) * x0 i k` mixes rows
of a row-stochastic `W`, so it keeps every row a distribution, and the shares
the settle reports, the mean row, are one too.
-/

open Finset

namespace ConsensusProofs

variable {n m : ℕ}

/-- Every row is a distribution over the options. -/
def RowsOnSimplex (x : Fin n → Fin m → ℝ) : Prop :=
  ∀ i, (∀ k, 0 ≤ x i k) ∧ ∑ k, x i k = 1

/-- One Friedkin–Johnsen step on every option at once. -/
def fjStepRows (W : Fin n → Fin n → ℝ) (s : Fin n → ℝ) (x0 x : Fin n → Fin m → ℝ) :
    Fin n → Fin m → ℝ :=
  fun i k => s i * ∑ j, W i j * x j k + (1 - s i) * x0 i k

/-- The step keeps every row a distribution. -/
theorem fjStepRows_onSimplex {W : Fin n → Fin n → ℝ} (hWnn : ∀ i j, 0 ≤ W i j)
    (hWsum : ∀ i, ∑ j, W i j = 1) {s : Fin n → ℝ} (hs : ∀ i, 0 ≤ s i ∧ s i ≤ 1)
    {x0 x : Fin n → Fin m → ℝ} (hx0 : RowsOnSimplex x0) (hx : RowsOnSimplex x) :
    RowsOnSimplex (fjStepRows W s x0 x) := by
  intro i
  refine ⟨fun k => ?_, ?_⟩
  · exact add_nonneg
      (mul_nonneg (hs i).1 (Finset.sum_nonneg fun j _ => mul_nonneg (hWnn i j) ((hx j).1 k)))
      (mul_nonneg (sub_nonneg.2 (hs i).2) ((hx0 i).1 k))
  · have inner : ∑ k, ∑ j, W i j * x j k = 1 := by
      rw [Finset.sum_comm]
      calc ∑ j, ∑ k, W i j * x j k = ∑ j, W i j * ∑ k, x j k := by
            refine Finset.sum_congr rfl fun j _ => ?_
            rw [Finset.mul_sum]
        _ = ∑ j, W i j := by
            refine Finset.sum_congr rfl fun j _ => ?_
            rw [(hx j).2, mul_one]
        _ = 1 := hWsum i
    simp only [fjStepRows]
    rw [Finset.sum_add_distrib, ← Finset.mul_sum, ← Finset.mul_sum, inner, (hx0 i).2]
    ring

/-- The shares, the mean row, sum to one. -/
theorem shares_sum_one (hn : 0 < n) {x : Fin n → Fin m → ℝ} (hx : RowsOnSimplex x) :
    ∑ k, (∑ i, x i k) / n = 1 := by
  rw [← Finset.sum_div, Finset.sum_comm]
  have : ∑ i : Fin n, ∑ k, x i k = n := by
    simp [fun i => (hx i).2]
  rw [this, div_self (by exact_mod_cast hn.ne')]

end ConsensusProofs
