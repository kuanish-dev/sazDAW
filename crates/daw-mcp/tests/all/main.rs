//! All integration tests of the crate are one test program: each separate one rebuilds and relinks the whole app stack,
//! and building the tests took minutes. Only tests with process-global state are kept separate.

mod http;
mod protocol;
