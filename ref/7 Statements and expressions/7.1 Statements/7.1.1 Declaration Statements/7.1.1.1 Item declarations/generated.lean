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

definition test.foo.bar : sem (unit) :=
let' ret ← ⋆;
return (ret)


definition test.foo : sem (unit) :=
dostep «$tmp» ← @test.foo.bar;
let' ret ← «$tmp»;
return (ret)


