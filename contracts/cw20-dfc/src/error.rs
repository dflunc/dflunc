use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("NotMinter: Sender {sender} is not minter.")]
    NotMinter {
        sender: String,
    },
}
