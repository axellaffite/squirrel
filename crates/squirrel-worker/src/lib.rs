//! Media job handlers: glue between Postgres, the blob store and
//! `squirrel-media`. Untrusted decoding goes through [`exec::Executor`].

pub mod exec;
