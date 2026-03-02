pub mod coordinator;
pub mod discovery;
pub mod worker;

pub mod proto {
    tonic::include_proto!("hyperchess.search");
}
