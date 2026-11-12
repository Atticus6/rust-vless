#![allow(clippy::result_large_err)]

//! 主节点与子节点共享的 gRPC 生成类型。

pub mod rpc {
    tonic::include_proto!("rustvless.rpc.v1");
}

pub use rpc::*;
