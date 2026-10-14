//! HANDLERS, values of three forms, each over the row `T` outside it, of the body's value `A` to its own `B` — as
//! okay-one's: answering in place (`answering`), with a state (`stateful`), taking the rest (`deep`). Typed here,
//! erased into a level of the machine.

use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::rc::Rc;

use crate::eff::{take, Answer, Eff, Node, Val};
use crate::machine::{AnsweringRt, DeepRt, K0, Level, StatefulRt};
use crate::row::Effect;

/// the types of a handler, the compiler's only
type Types<E, T, A, B> = PhantomData<fn() -> (E, T, A, B)>;

/// A HANDLER of `E`, over the row `T` outside it, of the body's `A` to `B`
pub struct Handler<E, T, A, B> {
    pub(crate) level: Level,
    _t: Types<E, T, A, B>,
}

/// THE REST of a handler taking it, data: resumed by an operation's `Reply`, as many times as wanted — each time its
/// levels put back as they were when it was taken, a state's included
pub struct K<T, B> {
    pub(crate) k: K0,
    _t: PhantomData<fn() -> (T, B)>,
}

impl<T, B> Clone for K<T, B> {
    fn clone(&self) -> Self {
        K { k: self.k.clone(), _t: PhantomData }
    }
}

impl<E: Effect, T, A: 'static, B: 'static> Handler<E, T, A, B> {
    /// ANSWERING IN PLACE: each operation its value at once, the rest never taken; `ret` of the body's value
    pub fn answering(value: impl Fn(E) -> Answer + 'static, ret: impl Fn(A) -> B + 'static) -> Self {
        let rt = AnsweringRt {
            value: Box::new(move |op: Box<dyn Any>| value(take_op::<E>(op)).0),
            ret: Box::new(move |v: Val| Box::new(ret(take::<A>(v))) as Val),
        };
        Handler { level: Level::Answering(TypeId::of::<E>(), Rc::new(rt)), _t: PhantomData }
    }

    /// WITH A STATE `S`, from `s0`: each operation its value and the state moved (`step`), in place; `ret` of the
    /// last state and the body's value. Taken into a rest, the state as it was then: each resumption its own
    pub fn stateful<S: Clone + 'static>(
        s0: S,
        step: impl Fn(&mut S, E) -> Answer + 'static,
        ret: impl Fn(S, A) -> B + 'static,
    ) -> Self {
        let rt = StatefulRt {
            step: Box::new(move |s: Rc<dyn Any>, op: Box<dyn Any>| {
                let mut s = state::<S>(s);
                // the state written in place when no rest holds it, copied when one does
                let Answer(v) = step(Rc::make_mut(&mut s), take_op::<E>(op));
                (s as Rc<dyn Any>, v)
            }),
            ret: Box::new(move |s: Rc<dyn Any>, v: Val| {
                let s = Rc::try_unwrap(state::<S>(s)).unwrap_or_else(|s| (*s).clone());
                Box::new(ret(s, take::<A>(v))) as Val
            }),
        };
        Handler { level: Level::Stateful(TypeId::of::<E>(), Rc::new(rt), Rc::new(s0)), _t: PhantomData }
    }

    /// TAKING THE REST: each operation its clause, given the rest `k` — resumed none, once or many times, by the
    /// operation's `Reply`; the clause a program over the row outside. `ret` of the body's value
    pub fn deep(
        ret: impl Fn(A) -> Eff<T, B> + 'static,
        apply: impl Fn(E, K<T, B>) -> Eff<T, B> + 'static,
    ) -> Self {
        let rt = DeepRt {
            ret: Box::new(move |v: Val| ret(take::<A>(v)).node),
            apply: Box::new(move |op: Box<dyn Any>, k: K0| -> Node {
                apply(take_op::<E>(op), K { k, _t: PhantomData }).node
            }),
        };
        Handler { level: Level::Deep(TypeId::of::<E>(), Rc::new(rt)), _t: PhantomData }
    }
}

/// an operation taken back as its effect: found by its effect's type id, it is one
fn take_op<E: 'static>(op: Box<dyn Any>) -> E {
    match op.downcast::<E>() {
        Ok(e) => *e,
        Err(_) => panic!("okay: an operation given to a handler of another effect"),
    }
}

/// a state taken back at its type
fn state<S: 'static>(s: Rc<dyn Any>) -> Rc<S> {
    match s.downcast::<S>() {
        Ok(s) => s,
        Err(_) => panic!("okay: a state of another type than its handler's"),
    }
}
