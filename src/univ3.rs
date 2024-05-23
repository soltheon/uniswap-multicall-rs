use anyhow::Result;
use ethers::prelude::*;

use crate::{V3_FACTORY, WETH};

abigen!(
    ERC20,
    r#"[
        function balanceOf(address addr) external view returns (uint)
    ]"#,
    event_derives(serde::Deserialize, serde::Serialize);
);

abigen!(
    UniswapV3Factory,
    r#"[
        function getPool(address tokenA, address tokenB, uint24 fee) external view returns (address pool)
    ]"#,
    event_derives(serde::Deserialize, serde::Serialize);

    UniswapV3Pair,
    r#"[
        function slot0() external view returns (uint160 sqrtPriceX96, int24 tick, uint16 observationIndex, uint16 observationCardinality, uint16 observationCardinalityNext, uint8 feeProtocol, bool unlocked)
    ]"#,
    event_derives(serde::Deserialize, serde::Serialize);
);

async fn get_v3_pairs(
    provider: &std::sync::Arc<Provider<Ws>>,
    multicall: &mut Multicall<Provider<Ws>>,
    tokens: &[Address],
) -> Result<Vec<Vec<Address>>> {
    let v3_factory =
        UniswapV3Factory::new(V3_FACTORY.parse::<Address>().unwrap(), provider.clone());

    for token in tokens {
        multicall.add_call(
            v3_factory.get_pool(*token, WETH.parse::<Address>().unwrap(), 500),
            false,
        );
        multicall.add_call(
            v3_factory.get_pool(*token, WETH.parse::<Address>().unwrap(), 3000),
            false,
        );
        multicall.add_call(
            v3_factory.get_pool(*token, WETH.parse::<Address>().unwrap(), 10000),
            false,
        );
    }

    let weth_pairs_v3: Vec<Address> = multicall.call_array().await?;
    multicall.clear_calls();

    let mut pairs: Vec<Vec<Address>> = Vec::new();
    for feePools in weth_pairs_v3.chunks(3) {
        println!("3 fee pools {:?}", feePools);
        pairs.push(feePools.to_vec());
    }
    Ok(pairs)
}

async fn get_v3_weth_reserves(
    _provider: &std::sync::Arc<Provider<Ws>>,
    _multicall: &mut Multicall<Provider<Ws>>,
    _pools: &Vec<Vec<Address>>,
    _tokens: &[Address],
) -> Result<Vec<u128>> {
    unimplemented!();
}
