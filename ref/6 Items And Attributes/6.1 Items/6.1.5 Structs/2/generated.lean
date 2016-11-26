import core.generated

noncomputable theory

open bool
open [class] classical
open [notation] function
open [class] int
open [notation] list
open [class] nat
open [notation] prod.ops
open [notation] unit

inductive test.Point :=
mk {} : i32 → i32 → test.Point

definition test.main : sem (unit) :=
let' p ← test.Point.mk (10 : int) (11 : int);
let' x ← match p with test.Point.mk x0 x1 := x0 end;
let' t4 ← x;
let' px ← t4;
let' ret ← ⋆;
return (⋆)


