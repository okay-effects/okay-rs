//! ROWS OF EFFECTS: a list of types, `Cons<E, R>` ending in `Nil` — `row![Reader<i64>, State<i64>]`. An effect's
//! place in a row found by the compiler (`Member`'s index, `Here` or `There`), as okay-one's `In`; the row without it
//! (`Rest`) what a handler of it leaves, as okay-one's `Removed`.

use std::marker::PhantomData;

/// the empty row: a program of no effect, run by `run`
pub struct Nil;

/// `E` first, then the row `R`
pub struct Cons<E, R>(PhantomData<fn() -> (E, R)>);

/// an effect of a row: the type of its operations, an enum — each operation a variant carrying its `Reply`
pub trait Effect: 'static {}

/// the place of an effect: first
pub struct Here;
/// the place of an effect: after the first, at `I` in the rest
pub struct There<I>(PhantomData<fn() -> I>);

/// `E` IN THE ROW `Self`, at `Ix` — the index found by the compiler, never written; `Rest` the row without it
#[diagnostic::on_unimplemented(
    message = "the effect `{E}` is not in the row `{Self}`",
    label = "no `{E}` in this row",
    note = "a program performs only the effects of its row; add `{E}` to the row, or handle it before"
)]
pub trait Member<E, Ix> {
    type Rest;
}

impl<E, R> Member<E, Here> for Cons<E, R> {
    type Rest = R;
}

impl<E, F, R, Ix> Member<E, There<Ix>> for Cons<F, R>
where
    R: Member<E, Ix>,
{
    type Rest = Cons<F, R::Rest>;
}

/// `row![A, B, C]`: `Cons<A, Cons<B, Cons<C, Nil>>>`
#[macro_export]
macro_rules! row {
    () => { $crate::Nil };
    ($e:ty $(, $rest:ty)* $(,)?) => { $crate::Cons<$e, $crate::row![$($rest),*]> };
}
