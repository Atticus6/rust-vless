#![allow(clippy::result_large_err)]

//! Generated RPC types shared by the master and child node.

pub mod rpc {
    tonic::include_proto!("rustvless.rpc.v1");
}

pub use rpc::*;
