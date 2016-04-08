import core.generated

open [class] classical
open core
open eq.ops
open nat
open option
open prod.ops

theorem loop
  {A : Type}
  (P Q : A → Prop) {R : A → A → Prop}
  {s : A} {l : (A → option A) → A → option A}
  (Hstart : P s)
  (Hdoes_step : ∀s f, P s → ∃s', l f s = some s' ∨ l f s = f s')
  (Hstep : ∀f s s', P s → l f s = f s' → P s')
  (Hstop : ∀f s s', P s → l f s = some s' → Q s')
  (Hwf_R : well_founded R)
  (HR : ∀f s s', P s → l f s = some s' → R s' s) :
  option.all Q (fix_opt l s) :=
have ∀(x : A), fix_opt.fix l R Hwf_R x ≠ none, from sorry,
obtain R' (HR' : some_opt (fix_opt.wf_R l) = some R') (Hwf_R' : (fix_opt.wf_R l) R'),
from some_opt.ex R (decidable.rec_on_true _ this),
sorry /-
begin
clear H_obtain_from,
apply option.all.intro,
{ apply bind_eq_some,
    { apply HR' },
    { unfold dite, apply decidable.rec_on, },
}
end -/

namespace core

theorem decidable_eq_decidable {A : Prop} (x y : decidable A) : x = y :=
begin
  cases x,
  { cases y,
    { apply rfl },
    { contradiction },
  },
  { cases y,
    { contradiction },
    { apply rfl }
  }
end

open ops

section
  parameters {Res : Type}
  abbreviation State := Res × Range ℕ
  parameters {P : ℕ → Res → Prop}
  parameters {l r : ℕ} {res₀ : Res}
  parameters {body : (State → option State) → ℕ → Res → Range ℕ → option State} {body' : ℕ → Res → Res}
  hypothesis (Hstart : P l res₀)
  hypothesis (Hdoes_step : ∀{f i res iter}, l ≤ i → i < r → P i res → body f i res iter = f ((body' i res, iter)))
  hypothesis (Hstep : ∀{i res}, l ≤ i → i < r → P i res → P (i+1) (body' i res))

  include Hstart Hdoes_step Hstep

  private definition variant (s : State) := Range.end_ s.2 - Range.start s.2

  inductive invariant (s : State) : Prop :=
  mk : Π(Hlo : l ≤ Range.start s.2)
  (Hhi : Range.start s.2 ≤ max l r)
  (Hend_ : Range.end_ s.2 = r)
  (HP : P (Range.start s.2) s.1), invariant s

  attribute num.u32.One [constructor]
  attribute ops.u32.Add [constructor]
  attribute cmp.impls.u32.PartialOrd [constructor]
  attribute iter.u32.Step [constructor]
  attribute iter.Step.to_PartialOrd [unfold 2]

  theorem loop_range : option.all (λstate, P (max l r) state.1) (fix_opt (λrec__ tmp__,
    match tmp__ with (res, iter) :=
      do tmp__ ← core.iter.ops.Range_A_.Iterator.next iter;
      match tmp__ with (t7, iter) :=
        match t7 with
        | core.option.Option.None := some (res, iter)
        | core.option.Option.Some i := body rec__ i res iter
        end
      end
    end) (res₀, ops.Range.mk l r)) :=
  begin
    apply loop invariant,
    {
      apply invariant.mk,
      all_goals esimp,
      { apply le_max_left },
      { apply Hstart },
    },
    { intro s f Hinv,
      cases Hinv, cases s with res iter,
      esimp at *,
      esimp [iter.ops.Range_A_.Iterator.next, cmp.PartialOrd.lt, mem.swap, num.u32.One.one, ops.u32.Add.add],
      esimp [cmp.impls.u32.PartialOrd.partial_cmp, cmp.impls.u32.Ord.cmp],
      cases classical.em (Range.start iter = Range.end_ iter) with Hend Hnot_end,
      { rewrite [if_pos Hend],
        esimp,
        rewrite [decidable_eq_decidable (classical.prop_decidable false) decidable_false, if_false],
        apply exists.intro,
        apply or.inl rfl,
      },
      { rewrite [if_neg Hnot_end],
        cases classical.em (Range.start iter < Range.end_ iter) with H₁ H₂,
        { rewrite [if_pos H₁],
          esimp,
          rewrite [decidable_eq_decidable (classical.prop_decidable true) decidable_true, if_true],
          apply exists.intro,
          apply or.inr,
          apply Hdoes_step Hlo (Hend_ ▸ H₁) HP
        },
        { rewrite [if_neg H₂],
          esimp,
          rewrite [decidable_eq_decidable (classical.prop_decidable false) decidable_false, if_false],
          apply exists.intro,
          apply or.inl rfl,
        }
      }
    },
    { intro f s s' Hinv,
      cases Hinv, cases s with res iter,
      esimp at *,
      esimp [iter.ops.Range_A_.Iterator.next, cmp.PartialOrd.lt, mem.swap, num.u32.One.one, ops.u32.Add.add],
      esimp [cmp.impls.u32.PartialOrd.partial_cmp, cmp.impls.u32.Ord.cmp],
      cases classical.em (Range.start iter = Range.end_ iter) with Hend Hnot_end,
      { rewrite [if_pos Hend],
        esimp,
        rewrite [decidable_eq_decidable (classical.prop_decidable false) decidable_false, if_false],
        esimp,
        intro Hfs',
      },
      { rewrite [if_neg Hnot_end],
      }
    }
end

end core
