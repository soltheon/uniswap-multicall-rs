use anyhow::Result;
use ethers::prelude::*;

pub mod univ2;
pub mod univ3;

pub const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
pub const V2_FACTORY: &str = "0x5C69bEe701ef814a2B6a3EDD4B1652CB9cc5aA6f";
pub const V2_ROUTER: &str = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D";
pub const V3_ROUTER: &str = "0xE592427A0AEce92De3Edee1F18E0157C05861564";
pub const V3_FACTORY: &str = "0x1F98431c8aD98523631AE4a59f267346ea31F984";

pub fn get_v2_factory(chain_id: u64) -> Result<Address> {
    match chain_id {
        1 => Ok(V2_FACTORY.parse::<Address>()?),
        11155111 => Ok("0x7E0987E5b3a30e3f2828572Bb659A548460a3003".parse::<Address>()?),
        _ => Err(anyhow::anyhow!("Unsupported chain id")),
    }
}

async fn get_weth_reserves(reserves: Vec<(u128, u128)>, tokens: Vec<Address>) -> Result<Vec<u128>> {
    let weth = WETH.parse::<Address>()?;

    let mut weth_reserves: Vec<u128> = Vec::new();

    for (i, reserve) in reserves.into_iter().enumerate() {
        if weth.lt(&tokens[i]) {
            weth_reserves.push(reserve.0);
        } else {
            weth_reserves.push(reserve.1);
        }
    }

    Ok(weth_reserves)
}
