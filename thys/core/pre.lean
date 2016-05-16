import data.nat data.list

open bool
open eq.ops
open nat
open option
open prod
open prod.ops
open sum

namespace nat
  definition of_int : ℤ → ℕ
  | (int.of_nat n) := n
  | _              := 0
end nat

namespace option
  variables {A B : Type}

  protected definition all [unfold 3] {A : Type} (P : A → Prop) : option A → Prop
  | (some x) := P x
  | none     := false

  protected definition bind [unfold 4] {A B : Type} (f : A → option B) : option A → option B
  | (some x) := f x
  | none     := none

  theorem all.intro (P : A → Prop) {x x'} (Hxx' : x = some x') (HPx' : P x') : option.all P x :=
  begin
    cases x,
    { apply option.no_confusion Hxx' },
    { esimp,
      injection Hxx',
      apply (a_eq⁻¹ ▸ HPx') }
  end

  theorem bind_eq_some {f : A → option B} {x x' y} (Hxx' : x = some x') (Hfx' : f x' = some y) : option.bind f x = some y :=
  begin
    cases x,
    { apply option.no_confusion Hxx' },
    { esimp,
      injection Hxx',
      apply (a_eq⁻¹ ▸ Hfx') }
  end

  theorem neq_none_of_eq_some {A : Type} {x : option A} {y : A} (H : x = some y) : x ≠ none :=
  begin
    cases x,
    { apply option.no_confusion H },
    { apply not.intro, apply option.no_confusion }
  end
end option

open option

notation `do` binder ` ← ` x `; ` r:(scoped f, option.bind f x) := r
notation `let` binder ` ← ` x `; ` r:(scoped f, f x) := r

definition sum.inl_opt {A B : Type} : sum A B → option A
| (inl a) := some a
| (inr _) := none

definition sum.inr_opt {A B : Type} : sum A B → option B
| (inl _) := none
| (inr b) := some b


namespace partial
infixr `⇀`:100 := λA B, A → option B

section
  parameters {A B : Type} {R : B → B → Prop}
  parameters (f : A ⇀ B)

  private definition R' : option B → option B → Prop
  | (some y) (some x) := R y x
  | _        _        := false

  private definition R'.wf (H : well_founded R) : well_founded R' :=
  begin
    apply well_founded.intro,
    intro x, cases x with x',
    { apply acc.intro,
      intro y,
      cases y; repeat contradiction },
    { induction (well_founded.apply H x') with x' _ ih,
      apply acc.intro,
      intro y HR', cases y with y',
      { contradiction },
      { apply ih _ HR' }
    }
  end

  parameter (R)
  definition inv_image (f : A ⇀ B) : A → A → Prop := inv_image R' f

  parameter {R}
  definition inv_image.wf (H : well_founded R) : well_founded (inv_image f) :=
  inv_image.wf f (R'.wf H)
end

end partial


open [class] classical

section
  parameters {State Res : Type}
  parameters (body : State → sum State Res)

  section
    parameter (R : State → State → Prop)

    private definition State' := sum State Res

    private definition R' : State' → State' → Prop
    | (inl s') (inl s) := R s' s
    | _        _       := false

    private noncomputable definition F (x : State') (f : Π (x' : State'), R' x' x → option State') : option State' :=
    do s ← sum.inl_opt x;
    match body s with
    | inr r := some (inr r)
    | x'    := if H : R' x' x then f x' H else none
    end

    private noncomputable definition fix (Hwf: well_founded R) (s : State) : option Res :=
    have well_founded R', from sorry,
    do x ← well_founded.fix F (inl s);
    sum.inr_opt x

    private noncomputable definition wf_R :=
    ∃Hwf : well_founded R, ∀s : State, fix Hwf s ≠ none
  end

  noncomputable definition loop (s : State) : option Res :=
  if Hex : ∃R, wf_R R then
    fix (classical.some Hex) (classical.some (classical.some_spec Hex)) s
  else none

  /-theorem loop_inv
    {A : Type}
    (P Q : A → Prop) {R : A → A → Prop}
    {s : A} {l : (A → option A) → A → option A}
    (Hstart : P s)
    (Hdoes_step : ∀s f, P s → ∃s', l f s = some s' ∨ l f s = f s')
    (Hstep : ∀f s s', P s → l f s = f s' → P s')
    (Hstop : ∀f s s', P s → l f s = some s' → Q s')
    (Hwf_R : well_founded R)
    (HR : ∀f s s', P s → l f s = some s' → R s' s) :
    option.all Q (fix_opt l s) :=-/

  theorem loop_eq
    {R : State → State → Prop}
    [Hwf_R : well_founded R]
    (HR : ∀s s', body s = inl s' → R s' s)
    {s : State} :
    loop s = match body s with
    | inl s' := loop s'
    | inr r  := some r
    end := sorry
  /-have Hex : ∃R, wf_R R,
  begin
    apply exists.intro R,
    apply exists.intro Hwf_R,
    intro s,
    exact @well_founded.induction State R Hwf_R _ s
    (begin
      intro s' Hind,
      rewrite [↑fix, well_founded.fix_eq, ↑F],
      note HR' := HR s',
      revert HR',
      cases body s' with s'' c,
      intro HR',
      cases c,
      { rewrite [dif_pos (HR' s'' rfl)],
        apply Hind _ (HR' s'' rfl) },
      { contradiction }
    end)
  end,
  /-obtain R' (HR' : some_opt loop.wf_R = some R') (Hloop_wf_R' : loop.wf_R R'),
  from some_opt.ex R (decidable.rec_on_true _ this),-/
  begin
    cases body s,
    rewrite [↑loop, ↑fix, dif_pos Hex, dif_pos Hex],
  end
    /-note Hwf_R' := dite_else_false Hloop_wf_R',
    rewrite [↑loop, HR', ▸*, dif_pos Hwf_R'],
    rewrite [↑loop.fix, well_founded.fix_eq, ↑loop.F],
    apply (congr_arg (prod.cases_on (body s))),
    apply funext, intro s',
    have R' s' s, from sorry,
    rewrite [dif_pos this, dif_pos Hwf_R'],
  end-/
  -/
end

section
  parameters {State Res : Type}
  parameters (body : State → option (sum State Res))

  noncomputable definition loop' (s : State) : option Res :=
  do res ← loop (λs, match body s with
  | some (inl s') := inl s'
  | some (inr r)  := inr (some r)
  | none          := inr none
  end) s;
  res
end

abbreviation u8 := nat
abbreviation u16 := nat
abbreviation u32 := nat
abbreviation u64 := nat
abbreviation usize := nat

abbreviation slice := list

definition checked.sub (n : nat) (m : nat) :=
if n ≥ m then some (n-m) else none

definition checked.div (n : nat) (m : nat) :=
if m ≠ 0 then some (mod n m) else none

definition checked.mod (n : nat) (m : nat) :=
if m ≠ 0 then some (mod n m) else none

/- TODO: actually check something -/
definition checked.shl (n : nat) (m : nat) := some (n * 2^m)
definition checked.shr (n : nat) (m : int) := some (div n (2^nat.of_int m))

namespace core
  definition intrinsics.add_with_overflow (n : nat) (m : nat) := some (n + m, false)

  definition mem.swap {T : Type} (x y : T) := some (unit.star,y,x)

  definition slice._T_.SliceExt.len {T : Type} (self : slice T) := some (list.length self)
  definition slice._T__SliceExt.get_unchecked {T : Type} (self : slice T) (index : usize) :=
  list.nth self index
end core
