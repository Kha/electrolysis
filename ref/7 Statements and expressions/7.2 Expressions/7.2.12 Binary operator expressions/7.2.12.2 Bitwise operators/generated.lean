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

definition test.unsigned (xₐ : u32) (yₐ : u32) : sem (unit) :=
let' «x$3» ← xₐ;
let' «y$4» ← yₐ;
let' t6 ← «x$3»;
let' t7 ← «y$4»;
let' t5 ← bitand u32.bits t6 t7;
let' t9 ← «x$3»;
let' t10 ← «y$4»;
let' t8 ← bitor u32.bits t9 t10;
let' t12 ← «x$3»;
let' t13 ← «y$4»;
let' t11 ← bitxor u32.bits t12 t13;
let' t15 ← «x$3»;
let' t16 ← «y$4»;
do «$tmp0» ← sem.map (λx, (x, tt)) (checked.shl u32.bits t15 t16);
let' t17 ← «$tmp0»;
let' t14 ← t17.1;
let' t19 ← «x$3»;
let' t20 ← «y$4»;
do «$tmp0» ← sem.map (λx, (x, tt)) (checked.shr u32.bits t19 t20);
let' t21 ← «$tmp0»;
let' t18 ← t21.1;
let' ret ← ⋆;
return (⋆)


definition test.signed (xₐ : i32) (yₐ : i32) : sem (unit) :=
let' «x$3» ← xₐ;
let' «y$4» ← yₐ;
let' t6 ← «x$3»;
let' t7 ← «y$4»;
let' t5 ← sbitand i32.bits t6 t7;
let' t9 ← «x$3»;
let' t10 ← «y$4»;
let' t8 ← sbitor i32.bits t9 t10;
let' t12 ← «x$3»;
let' t13 ← «y$4»;
let' t11 ← sbitxor i32.bits t12 t13;
let' t15 ← «x$3»;
let' t16 ← «y$4»;
do «$tmp0» ← sem.map (λx, (x, tt)) (checked.sshls i32.bits t15 t16);
let' t17 ← «$tmp0»;
let' t14 ← t17.1;
let' t19 ← «x$3»;
let' t20 ← «y$4»;
do «$tmp0» ← sem.map (λx, (x, tt)) (checked.sshrs i32.bits t19 t20);
let' t21 ← «$tmp0»;
let' t18 ← t21.1;
let' ret ← ⋆;
return (⋆)


definition test.bool (xₐ : bool) (yₐ : bool) : sem (unit) :=
let' «x$3» ← xₐ;
let' «y$4» ← yₐ;
let' t6 ← «x$3»;
let' t7 ← «y$4»;
let' t5 ← band t6 t7;
let' t9 ← «x$3»;
let' t10 ← «y$4»;
let' t8 ← bor t9 t10;
let' t12 ← «x$3»;
let' t13 ← «y$4»;
let' t11 ← bxor t12 t13;
let' ret ← ⋆;
return (⋆)


