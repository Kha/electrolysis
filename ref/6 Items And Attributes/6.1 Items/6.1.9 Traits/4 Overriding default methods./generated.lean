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

structure test.Foo [class] (Self : Type₁) :=
(bar : (Self → sem (unit)))

definition test.Foo.baz {Self : Type₁} [«test.Foo Self» : test.Foo Self] (selfₐ : Self) : sem (unit) :=
let' «self$2» ← selfₐ;
let' t3 ← «self$2»;
dostep «$tmp» ← @test.Foo.bar Self «test.Foo Self» t3;
let' ret ← «$tmp»;
return (⋆)


structure test.Bar := mk {} ::

definition test.«test.Bar as test.Foo».bar (selfₐ : (test.Bar)) : sem (unit) :=
let' «self$2» ← selfₐ;
let' ret ← ⋆;
return (⋆)


/- test.«test.Bar as test.Foo».baz: unimplemented: overriding default method |test.«test.Bar as test.Foo».baz -/

/- test.«test.Bar as test.Foo»: failed dependencies |test.«test.Bar as test.Foo».baz -/

