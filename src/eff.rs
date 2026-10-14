//! A PROGRAM OF EFFECTS, `Eff<R, A>`: of the value `A`, its effects the row `R` — data, a tree of nodes the machine
//! runs. The types only the compiler's: the nodes hold values erased (`Box<dyn Any>`), each taken back at the type it
//! was made at, which the typed API makes the only one possible.

use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::rc::Rc;

use crate::handler::Handler;
use crate::row::{Effect, Member, Nil};

/// a value on its way, erased
pub(crate) type Val = Box<dyn Any>;
/// a frame: the rest of a bind, given the value — `Fn`, not `FnOnce`: a `k` is resumed as many times as its handler
/// wants
pub(crate) type Frame = Rc<dyn Fn(Val) -> Node>;

/// a program erased: the machine's
#[doc(hidden)]
pub struct Node(pub(crate) Kind);

pub(crate) enum Kind {
    /// a value
    Return(Val),
    /// a program, then the rest given its value
    Bind(Box<Node>, Frame),
    /// an operation of the effect whose type id is given: its handler the nearest of that effect
    Perform(TypeId, Box<dyn Any>),
    /// a handler entered: its level, the body under it
    Handle(Box<Node>, crate::machine::Level),
    /// a `k` resumed: its levels put back, the value to its frames
    Resume(crate::machine::K0, Val),
}

/// A PROGRAM OF EFFECTS over the row `R`, of the value `A`
pub struct Eff<R, A> {
    pub(crate) node: Node,
    _t: PhantomData<fn() -> (R, A)>,
}

impl<R, A> Eff<R, A> {
    pub(crate) fn of(kind: Kind) -> Self {
        Eff { node: Node(kind), _t: PhantomData }
    }
}

/// what a program is to the machine: its node, and a program back from one
pub trait Program: Sized {
    #[doc(hidden)]
    fn into_node(self) -> Node;
    #[doc(hidden)]
    fn from_node(n: Node) -> Self;
}

impl<R, A> Program for Eff<R, A> {
    fn into_node(self) -> Node {
        self.node
    }
    fn from_node(n: Node) -> Self {
        Eff { node: n, _t: PhantomData }
    }
}

/// THE ANSWERS MET at a bind, by the kind of the rest: a program of effects after a program of effects, one of
/// effects. (Stage 2: a program moving the answer, `Cont`, each of the four pairs its own impl — none overlapping,
/// as `Eff` and `Cont` are types of their own.)
#[diagnostic::on_unimplemented(message = "a `{Self}` cannot be followed by a `{N}`: the rows or the answers do not meet")]
pub trait Bind<N> {
    type Out: Program;
}

impl<R, A, B> Bind<Eff<R, B>> for Eff<R, A> {
    type Out = Eff<R, B>;
}

/// `a`, over any row
pub fn pure<R, A: 'static>(a: A) -> Eff<R, A> {
    Eff::of(Kind::Return(Box::new(a)))
}

/// AN OPERATION of `E`, in the row `R`: `make` its variant given its reply — `perform(Reader::Ask)`, `perform(|r|
/// State::Put(s, r))`; the program's value what its handler gives back
pub fn perform<R, E: Effect, Ix, T: 'static>(make: impl FnOnce(Reply<T>) -> E) -> Eff<R, T>
where
    R: Member<E, Ix>,
{
    Eff::of(Kind::Perform(TypeId::of::<E>(), Box::new(make(Reply(PhantomData)))))
}

impl<R, A: 'static> Eff<R, A> {
    /// `f` given the value: its program the rest — at the answers met (`Bind`)
    pub fn flat_map<N: Program, F>(self, f: F) -> <Self as Bind<N>>::Out
    where
        F: Fn(A) -> N + 'static,
        Self: Bind<N>,
    {
        let frame: Frame = Rc::new(move |v: Val| f(take::<A>(v)).into_node());
        <Self as Bind<N>>::Out::from_node(Node(Kind::Bind(Box::new(self.node), frame)))
    }

    /// `f` of the value
    pub fn map<B: 'static>(self, f: impl Fn(A) -> B + 'static) -> Eff<R, B> {
        let frame: Frame = Rc::new(move |v: Val| Node(Kind::Return(Box::new(f(take::<A>(v))))));
        Eff::of(Kind::Bind(Box::new(self.node), frame))
    }

    /// `E` handled by `h`, wherever it is in the row: the row without it, `T`, the handler's
    pub fn handle<E: Effect, Ix, B: 'static>(self, h: Handler<E, R::Rest, A, B>) -> Eff<R::Rest, B>
    where
        R: Member<E, Ix>,
    {
        Eff::of(Kind::Handle(Box::new(self.node), h.level))
    }
}

impl<A: 'static> Eff<Nil, A> {
    /// the value: a program of no effect run
    pub fn run(self) -> A {
        take::<A>(crate::machine::run(self.node))
    }
}

/// a value taken back at its type: the type it was made at — the typed API allows no other
pub(crate) fn take<A: 'static>(v: Val) -> A {
    match v.downcast::<A>() {
        Ok(a) => *a,
        Err(_) => panic!("okay: a value taken at another type than it was made at — a reply given to another operation"),
    }
}

/// THE REPLY to an operation: the only way to its value — `give` it (a handler answering in place), or `resume` the
/// rest with it (a handler taking the rest). Made by `perform` alone, one for each operation
pub struct Reply<T>(PhantomData<fn(T) -> T>);

impl<T> Clone for Reply<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Reply<T> {}

/// the value given to an operation, erased: made by a `Reply`
pub struct Answer(pub(crate) Val);

impl<T: 'static> Reply<T> {
    /// the operation's value `t`
    pub fn give(self, t: T) -> Answer {
        Answer(Box::new(t))
    }

    /// the rest `k` resumed with `t`: a program over the row outside the handler, of its answer
    pub fn resume<R, B>(self, k: &crate::handler::K<R, B>, t: T) -> Eff<R, B> {
        Eff::of(Kind::Resume(k.k.clone(), Box::new(t)))
    }
}

/// no value: an operation that never comes back (`fail`, `raise`)
pub enum Never {}
