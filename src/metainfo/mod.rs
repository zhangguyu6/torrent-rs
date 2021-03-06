//! This moduie implements metainfo file format defined in https://www.bittorrent.org/beps/bep_0003.html

mod address;
pub use address::PeerAddress;
pub(crate) use address::{ADDRESS_V4_LEN, ADDRESS_V6_LEN};

mod error;
pub use error::Error;

mod info;
pub use info::Info;

mod metainfo;
pub use metainfo::{MetaInfo, UrlList};

mod piece;
pub(crate) use piece::ID_LEN;
pub use piece::{
    HashPiece, HashPieces, PIECE_SIZE_1M, PIECE_SIZE_256_KB, PIECE_SIZE_2M, PIECE_SIZE_512_KB,
};
