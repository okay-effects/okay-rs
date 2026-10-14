//! THE MACHINE: a loop over the program's nodes, a stack of levels — each a handler's (or the top), its frames the
//! binds waiting inside it. An operation goes to the nearest level of its effect: answered in place, the state's moved
//! in place, or — a handler taking the rest — the levels from it up taken off as the rest `k`, data, and its clause
//! run outside them. A `k` resumed: its levels pushed back, copies, each state as it was. No recursion: binds nested
//! to any depth, in a stack of the heap.

use std::any::{Any, TypeId};
use std::rc::Rc;

use crate::eff::{Frame, Kind, Node, Val};

pub(crate) struct AnsweringRt {
    pub(crate) value: Box<dyn Fn(Box<dyn Any>) -> Val>,
    pub(crate) ret: Box<dyn Fn(Val) -> Val>,
}

/// a state's step: the state and the operation to the state moved and the value
pub(crate) type Step = Box<dyn Fn(Rc<dyn Any>, Box<dyn Any>) -> (Rc<dyn Any>, Val)>;

pub(crate) struct StatefulRt {
    pub(crate) step: Step,
    pub(crate) ret: Box<dyn Fn(Rc<dyn Any>, Val) -> Val>,
}

pub(crate) struct DeepRt {
    pub(crate) ret: Box<dyn Fn(Val) -> Node>,
    pub(crate) apply: Box<dyn Fn(Box<dyn Any>, K0) -> Node>,
}

/// a level: the top, or a handler's — its effect's type id, its runtime, a state's register
#[derive(Clone)]
pub(crate) enum Level {
    Top,
    Answering(TypeId, Rc<AnsweringRt>),
    Stateful(TypeId, Rc<StatefulRt>, Rc<dyn Any>),
    Deep(TypeId, Rc<DeepRt>),
}

impl Level {
    fn effect(&self) -> Option<TypeId> {
        match self {
            Level::Top => None,
            Level::Answering(e, _) | Level::Stateful(e, _, _) | Level::Deep(e, _) => Some(*e),
        }
    }
}

/// a level and the frames waiting inside it, innermost last
#[derive(Clone)]
pub(crate) struct Segment {
    level: Level,
    frames: Vec<Frame>,
}

/// a rest taken: the levels from a handler's up, outermost first — shared, resumed by copies
#[derive(Clone)]
pub(crate) struct K0(Rc<Vec<Segment>>);

/// a program of no effect run to its value
pub(crate) fn run(node: Node) -> Val {
    let mut stack: Vec<Segment> = vec![Segment { level: Level::Top, frames: Vec::new() }];
    // in a state's place while it is moved: the state taken out, so that no one else holds it and it is written in place
    let hole: Rc<dyn Any> = Rc::new(());
    let mut cur = node.0;
    loop {
        cur = match cur {
            Kind::Return(v) => {
                let Some(top) = stack.last_mut() else { return v };
                match top.frames.pop() {
                    Some(f) => f(v).0,
                    // a level's frames all done: the level left, the value through its `ret`
                    None => match stack.pop().map(|s| s.level) {
                        None | Some(Level::Top) => return v,
                        Some(Level::Answering(_, rt)) => Kind::Return((rt.ret)(v)),
                        Some(Level::Stateful(_, rt, s)) => Kind::Return((rt.ret)(s, v)),
                        Some(Level::Deep(_, rt)) => (rt.ret)(v).0,
                    },
                }
            }
            Kind::Bind(m, f) => {
                if let Some(top) = stack.last_mut() {
                    top.frames.push(f);
                }
                m.0
            }
            Kind::Handle(body, level) => {
                stack.push(Segment { level, frames: Vec::new() });
                body.0
            }
            Kind::Perform(e, op) => {
                // the nearest level of the effect: there is one — the row says so, and `run` takes no row but `Nil`
                let Some(i) = stack.iter().rposition(|s| s.level.effect() == Some(e)) else {
                    panic!("okay: an operation with no handler — a row not checked")
                };
                match &mut stack[i].level {
                    Level::Answering(_, rt) => Kind::Return((rt.value)(op)),
                    Level::Stateful(_, rt, s) => {
                        let (s2, v) = (rt.step)(std::mem::replace(s, hole.clone()), op);
                        *s = s2;
                        Kind::Return(v)
                    }
                    Level::Deep(_, rt) => {
                        let rt = rt.clone();
                        let k = K0(Rc::new(stack.split_off(i)));
                        (rt.apply)(op, k).0
                    }
                    Level::Top => panic!("okay: the top handles no effect"),
                }
            }
            Kind::Resume(k, v) => {
                stack.extend(k.0.iter().cloned());
                Kind::Return(v)
            }
        }
    }
}
