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

definition test.main (xₐ : bool) (yₐ : bool) : sem (unit) :=
let' «x$3» ← xₐ;
let' «y$4» ← yₐ;
let' t6 ← «x$3»;
if t6 = bool.tt then
let' t7 ← «y$4»;
if t7 = bool.tt then
let' t5 ← tt;
let' t9 ← «x$3»;
if t9 = bool.tt then
let' t8 ← tt;
let' ret ← ⋆;
return (⋆)
else
let' t10 ← «y$4»;
if t10 = bool.tt then
let' t8 ← tt;
let' ret ← ⋆;
return (⋆)
else
let' t8 ← ff;
let' ret ← ⋆;
return (⋆)
else
let' t5 ← ff;
let' t9 ← «x$3»;
if t9 = bool.tt then
let' t8 ← tt;
let' ret ← ⋆;
return (⋆)
else
let' t10 ← «y$4»;
if t10 = bool.tt then
let' t8 ← tt;
let' ret ← ⋆;
return (⋆)
else
let' t8 ← ff;
let' ret ← ⋆;
return (⋆)
else
let' t5 ← ff;
let' t9 ← «x$3»;
if t9 = bool.tt then
let' t8 ← tt;
let' ret ← ⋆;
return (⋆)
else
let' t10 ← «y$4»;
if t10 = bool.tt then
let' t8 ← tt;
let' ret ← ⋆;
return (⋆)
else
let' t8 ← ff;
let' ret ← ⋆;
return (⋆)


