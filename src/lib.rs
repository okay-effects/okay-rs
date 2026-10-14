//! # okay-rs
//!
//! okay-one in Rust: a program of effects is DATA (`Eff<R, A>`, a tree of nodes), its effects a ROW (`R`, a list of
//! types), its handlers VALUES of three forms not tied to it (answering in place, with a state, taking the rest), the
//! rest `k` data too — resumed as many times as a handler wants, each time as it was taken. A machine runs it.
//!
//! ```
//! use okay::*;
//! use okay::effects::*;
//!
//! type R = row![Reader<i64>, State<i64>];
//! let p: Eff<R, i64> = eff! { data;
//!     n <- ask::<i64, _, _>();
//!     _ <- modify::<i64, _, _>(move |s| s + n);
//!     s <- get::<i64, _, _>();
//!     yield s * 2
//! };
//! assert_eq!(p.handle(reader(5)).handle(state(1)).run(), (6, 12));
//! ```
//!
//! A program runs only when every effect of its row is handled:
//!
//! ```compile_fail
//! use okay::*;
//! use okay::effects::*;
//! let p: Eff<row![Reader<i64>], i64> = ask::<i64, _, _>();
//! p.run();
//! ```
//!
//! and performs only the effects of its row:
//!
//! ```compile_fail
//! use okay::*;
//! use okay::effects::*;
//! let p: Eff<row![Reader<i64>], i64> = get::<i64, _, _>();
//! ```

pub mod effects;
mod eff;
mod frame;
mod handler;
mod machine;
mod row;
mod val;

pub mod audit;
pub mod direct;
pub mod typed;

pub use eff::{defer, perform, pure, tail_rec, Loop, Answer, AnyOp, Bind, Eff, Never, Node, Pass, Program, Reply};
pub use handler::{Handler, Interceptor, Observer, K};
pub use row::{AuditAll, Audited, Cons, Effect, Here, Kind, Mark, MarkKind, Member, Nil, Plain, PlainKind, Sensitive, There};

/// A TAIL CALL: `call!(f(a, b))` — `f(a, b)` made when the machine gets to it, not now; so functions calling each
/// other in tail position (`is_even` → `is_odd` → …) run in constant stack, however deep. The arguments are evaluated
/// when the call is made, and cloned: a rest resumed twice calls again
#[macro_export]
macro_rules! call {
    ($($f:ident)::+ ( $($arg:expr),* $(,)? )) => {
        $crate::defer(move || $($f)::+($(($arg).clone()),*))
    };
}

/// A FOR OVER PROGRAMS, one API: `x <- p;` binds `p`'s value, `_ <- p;` or `p;` drops it, `let x = v;` names a
/// value, the last line a program or `yield v`.
///
/// `eff! { … }` is an `async` block (`okay::typed`): compiled for the handlers it runs under — an operation answered
/// in place a call, one dropping the rest the block dropped, one taking it once the block suspended.
///
/// `eff! { data; … }` is a program as data (`Eff`, the machine): for a rest taken many times (`choose`). Its rests
/// are closures called as many times as a handler resumes them: each takes copies (`clone`) of the names bound before
/// it — so a name bound is `Clone`.
#[macro_export]
macro_rules! eff {
    // A PROGRAM AS DATA, the machine's
    (data; $($t:tt)+) => { $crate::eff!(@data [] $($t)+) };
    (@data [$($b:ident)*] yield $e:expr) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        $crate::pure($e)
    }};
    (@data [$($b:ident)*] _ <- $e:expr ; $($rest:tt)+) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        ($e).flat_map(move |_| $crate::eff!(@data [$($b)*] $($rest)+))
    }};
    (@data [$($b:ident)*] let $x:ident = $v:expr ; $($rest:tt)+) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        let $x = $v;
        $crate::eff!(@data [$($b)* $x] $($rest)+)
    }};
    (@data [$($b:ident)*] $x:ident <- $e:expr ; $($rest:tt)+) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        ($e).flat_map(move |$x| $crate::eff!(@data [$($b)* $x] $($rest)+))
    }};
    (@data [$($b:ident)*] $e:expr ; $($rest:tt)+) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        ($e).flat_map(move |_| $crate::eff!(@data [$($b)*] $($rest)+))
    }};
    (@data [$($b:ident)*] $e:expr) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        $e
    }};
    // COMPILED BY THE HANDLERS' TYPES: an async block
    (@async yield $e:expr) => { $e };
    (@async _ <- $e:expr ; $($rest:tt)+) => {{ ($e).await; $crate::eff!(@async $($rest)+) }};
    (@async let $x:ident = $v:expr ; $($rest:tt)+) => {{ let $x = $v; $crate::eff!(@async $($rest)+) }};
    (@async $x:ident <- $e:expr ; $($rest:tt)+) => {{ let $x = ($e).await; $crate::eff!(@async $($rest)+) }};
    (@async $e:expr ; $($rest:tt)+) => {{ ($e).await; $crate::eff!(@async $($rest)+) }};
    (@async $e:expr) => { ($e).await };
    ($($t:tt)+) => { async move { $crate::eff!(@async $($t)+) } };
}
