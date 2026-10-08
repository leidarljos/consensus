import Mathlib

/-!
# The Friedkin–Johnsen step is a contraction

`ljos-consensus` settles a vote by iterating, one option's column at a time,

  `x(t+1) i = s i * ∑ j, W i j * x(t) j + (1 - s i) * x0 i`

over a row-stochastic trust matrix `W` and susceptibilities `s i ∈ [0, q]`.

* `fjStep_dist_le`: the step is `q`-Lipschitz in the sup norm, since each row
  of `W` averages.
* `fjStep_contracting`: for `q < 1` it is a contraction, so it has exactly one
  fixed point and the iteration converges to it from any start (Banach).
* `fjStep_aposteriori`: Banach's part of the bound `settle_with` stops on,
  `dist x(t) x* ≤ q / (1 - q) * dist x(t-1) x(t)`, without the rounding term
  derive/sollya/rounding.sollya adds.
-/

open Finset
open scoped NNReal Topology

namespace ConsensusProofs

variable {n : ℕ}

/-- A row-stochastic trust matrix: nonnegative rows that sum to one. -/
structure RowStochastic (W : Fin n → Fin n → ℝ) : Prop where
  nonneg : ∀ i j, 0 ≤ W i j
  row_sum : ∀ i, ∑ j, W i j = 1

/-- One Friedkin–Johnsen step on one option's column. -/
def fjStep (W : Fin n → Fin n → ℝ) (s x0 : Fin n → ℝ) (x : Fin n → ℝ) : Fin n → ℝ :=
  fun i => s i * ∑ j, W i j * x j + (1 - s i) * x0 i

/-- A row of `W` averages: it moves no entry farther than the sup distance. -/
theorem row_average_le {W : Fin n → Fin n → ℝ} (hW : RowStochastic W)
    (x y : Fin n → ℝ) (i : Fin n) :
    |∑ j, W i j * x j - ∑ j, W i j * y j| ≤ dist x y := by
  rw [← Finset.sum_sub_distrib]
  calc |∑ j, (W i j * x j - W i j * y j)|
      ≤ ∑ j, |W i j * x j - W i j * y j| := Finset.abs_sum_le_sum_abs _ _
    _ = ∑ j, W i j * |x j - y j| := by
        refine Finset.sum_congr rfl fun j _ => ?_
        rw [← mul_sub, abs_mul, abs_of_nonneg (hW.nonneg i j)]
    _ ≤ ∑ j, W i j * dist x y := by
        refine Finset.sum_le_sum fun j _ => ?_
        refine mul_le_mul_of_nonneg_left ?_ (hW.nonneg i j)
        rw [← Real.dist_eq]
        exact dist_le_pi_dist x y j
    _ = dist x y := by rw [← Finset.sum_mul, hW.row_sum i, one_mul]

/-- The step is `q`-Lipschitz in the sup norm when every susceptibility is
in `[0, q]`. -/
theorem fjStep_dist_le {W : Fin n → Fin n → ℝ} (hW : RowStochastic W) {s : Fin n → ℝ}
    {q : ℝ} (hq : 0 ≤ q) (hs : ∀ i, 0 ≤ s i ∧ s i ≤ q) (x0 x y : Fin n → ℝ) :
    dist (fjStep W s x0 x) (fjStep W s x0 y) ≤ q * dist x y := by
  refine (dist_pi_le_iff (mul_nonneg hq dist_nonneg)).2 fun i => ?_
  rw [Real.dist_eq]
  have h : fjStep W s x0 x i - fjStep W s x0 y i
      = s i * (∑ j, W i j * x j - ∑ j, W i j * y j) := by
    simp only [fjStep]; ring
  rw [h, abs_mul, abs_of_nonneg (hs i).1]
  calc s i * |∑ j, W i j * x j - ∑ j, W i j * y j| ≤ s i * dist x y :=
        mul_le_mul_of_nonneg_left (row_average_le hW x y i) (hs i).1
    _ ≤ q * dist x y := mul_le_mul_of_nonneg_right (hs i).2 dist_nonneg

/-- With every susceptibility at most `q < 1` the step is a contraction. -/
theorem fjStep_contracting {W : Fin n → Fin n → ℝ} (hW : RowStochastic W) {s : Fin n → ℝ}
    {q : ℝ≥0} (hq : q < 1) (hs : ∀ i, 0 ≤ s i ∧ s i ≤ q) (x0 : Fin n → ℝ) :
    ContractingWith q (fjStep W s x0) :=
  ⟨hq, LipschitzWith.of_dist_le_mul fun x y => fjStep_dist_le hW q.2 hs x0 x y⟩

/-- The settle: the unique fixed point of the step. -/
noncomputable def settle {W : Fin n → Fin n → ℝ} (hW : RowStochastic W) {s : Fin n → ℝ}
    {q : ℝ≥0} (hq : q < 1) (hs : ∀ i, 0 ≤ s i ∧ s i ≤ q) (x0 : Fin n → ℝ) : Fin n → ℝ :=
  ContractingWith.fixedPoint (fjStep W s x0) (fjStep_contracting hW hq hs x0)

/-- The settle is a fixed point of the step. -/
theorem settle_isFixedPt {W : Fin n → Fin n → ℝ} (hW : RowStochastic W) {s : Fin n → ℝ}
    {q : ℝ≥0} (hq : q < 1) (hs : ∀ i, 0 ≤ s i ∧ s i ≤ q) (x0 : Fin n → ℝ) :
    Function.IsFixedPt (fjStep W s x0) (settle hW hq hs x0) :=
  ContractingWith.fixedPoint_isFixedPt _

/-- It is the only one. -/
theorem settle_unique {W : Fin n → Fin n → ℝ} (hW : RowStochastic W) {s : Fin n → ℝ}
    {q : ℝ≥0} (hq : q < 1) (hs : ∀ i, 0 ≤ s i ∧ s i ≤ q) (x0 x : Fin n → ℝ)
    (hx : Function.IsFixedPt (fjStep W s x0) x) : x = settle hW hq hs x0 :=
  ContractingWith.fixedPoint_unique _ hx

/-- The iteration converges to the settle from any start. -/
theorem iterate_tendsto_settle {W : Fin n → Fin n → ℝ} (hW : RowStochastic W)
    {s : Fin n → ℝ} {q : ℝ≥0} (hq : q < 1) (hs : ∀ i, 0 ≤ s i ∧ s i ≤ q)
    (x0 x : Fin n → ℝ) :
    Filter.Tendsto (fun t => (fjStep W s x0)^[t] x) Filter.atTop (𝓝 (settle hW hq hs x0)) :=
  ContractingWith.tendsto_iterate_fixedPoint _ x

/-- The bound the iteration stops on: one step from `x`, the distance to the
settle is at most `q / (1 - q)` times the length of that step. -/
theorem fjStep_aposteriori {W : Fin n → Fin n → ℝ} (hW : RowStochastic W) {s : Fin n → ℝ}
    {q : ℝ≥0} (hq : q < 1) (hs : ∀ i, 0 ≤ s i ∧ s i ≤ q) (x0 x : Fin n → ℝ) :
    dist (fjStep W s x0 x) (settle hW hq hs x0)
      ≤ q / (1 - q) * dist x (fjStep W s x0 x) := by
  have hf := fjStep_contracting hW hq hs x0
  set T := fjStep W s x0
  have hpos : (0 : ℝ) < 1 - q := sub_pos.2 (by exact_mod_cast hq)
  have h1 : dist (T x) (settle hW hq hs x0) ≤ dist (T x) (T (T x)) / (1 - q) :=
    hf.dist_fixedPoint_le (T x)
  have h2 : dist (T x) (T (T x)) ≤ q * dist x (T x) := hf.2.dist_le_mul x (T x)
  calc dist (T x) (settle hW hq hs x0) ≤ dist (T x) (T (T x)) / (1 - q) := h1
    _ ≤ q * dist x (T x) / (1 - q) := div_le_div_of_nonneg_right h2 hpos.le
    _ = q / (1 - q) * dist x (T x) := by ring

end ConsensusProofs
