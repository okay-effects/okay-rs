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
//! let p: Eff<R, i64> = eff! {
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
mod handler;
mod machine;
mod row;

pub use eff::{perform, pure, Answer, Bind, Eff, Never, Node, Program, Reply};
pub use handler::{Handler, K};
pub use row::{Cons, Effect, Here, Member, Nil, There};

/// A FOR OVER PROGRAMS: `x <- p;` binds `p`'s value, `_ <- p;` or `p;` drops it, `let x = v;` names a value, the last
/// line a program or `yield v`. The rests are closures called as many times as a handler resumes them: each takes
/// copies (`clone`) of the names bound before it — so a name bound is `Clone`.
#[macro_export]
macro_rules! eff {
    (@ [$($b:ident)*] yield $e:expr) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        $crate::pure($e)
    }};
    (@ [$($b:ident)*] _ <- $e:expr ; $($rest:tt)+) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        ($e).flat_map(move |_| $crate::eff!(@ [$($b)*] $($rest)+))
    }};
    (@ [$($b:ident)*] let $x:ident = $v:expr ; $($rest:tt)+) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        let $x = $v;
        $crate::eff!(@ [$($b)* $x] $($rest)+)
    }};
    (@ [$($b:ident)*] $x:ident <- $e:expr ; $($rest:tt)+) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        ($e).flat_map(move |$x| $crate::eff!(@ [$($b)* $x] $($rest)+))
    }};
    (@ [$($b:ident)*] $e:expr ; $($rest:tt)+) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        ($e).flat_map(move |_| $crate::eff!(@ [$($b)*] $($rest)+))
    }};
    (@ [$($b:ident)*] $e:expr) => {{
        $( #[allow(unused_variables, clippy::redundant_clone)] let $b = $b.clone(); )*
        $e
    }};
    ($($t:tt)+) => { $crate::eff!(@ [] $($t)+) };
}
