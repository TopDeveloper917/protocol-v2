use solana_program::pubkey::Pubkey;

#[derive(Debug, PartialEq, Eq)]
pub enum PerpFulfillmentMethod {
    AMM(Option<u64>),
    Match(Pubkey, usize),
}

#[derive(Debug)]
pub enum SpotFulfillmentMethod {
    SerumV3,
    Match,
}
