use anyhow::Result;
use ethers::prelude::*;

use crate::{get_v2_factory, V2_ROUTER, WETH};

abigen!(
    ERC20,
    r#"[
        function balanceOf(address addr) external view returns (uint)
    ]"#,
    event_derives(serde::Deserialize, serde::Serialize);
);

abigen!(
    UniswapV2Factory,
    r#"[
        function getPair(address tokenA, address tokenB) external view returns (address pair)
    ]"#,
    event_derives(serde::Deserialize, serde::Serialize);

    UniswapV2Pair,
    r#"[
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast)
    ]"#,
    event_derives(serde::Deserialize, serde::Serialize);

    UniswapV2Router02,
    r#"[
        function getAmountOut(uint amountIn, uint reserveIn, uint reserveOut) external pure returns (uint amountOut)
    ]"#,
    event_derives(serde::Deserialize, serde::Serialize);
);

/*  will fetch all pairs created with the provided token
 *  it will try to use a larger and larger block span per request until it fails
 *  when it fails it will use much smaller block span until it succeeds
 *  returns [ (token0, token1, pair) ]
 *
*/
pub async fn get_pairs_with_token(
    provider: &std::sync::Arc<Provider<Ws>>,
    token: Address,
    from_block: u64,
    to_block: Option<u64>,
) -> Result<Vec<(Address, Address, Address)>> {
    let to_block = if let Some(to_block) = to_block {
        to_block
    } else {
        provider.get_block_number().await?.as_u64()
    };

    if from_block > to_block {
        return Err(anyhow::anyhow!("Invalid block range"));
    }

    let v2_factory = get_v2_factory(provider.get_chainid().await?.as_u64())?;

    let mut results: Vec<(Address, Address, Address)> = Vec::new();

    let mut max_block_range = 1_000_000;

    let mut current_block = from_block;

    while current_block < to_block {
        let current_to_block =
            (current_block + (to_block - current_block)).min(max_block_range + current_block);

        println!(
            "searching for pairs from {} to {}",
            current_block, current_to_block
        );

        // event PairCreated(address indexed token0, address indexed token1, address pair, uint);
        let pair_created_filter = Filter::new()
            .address(v2_factory)
            .from_block(current_block)
            .to_block(current_to_block)
            .event("PairCreated(address,address,address,uint256)")
            .topic1(token);

        let pair_created_filter_2 = Filter::new()
            .address(v2_factory)
            .from_block(current_block)
            .to_block(current_to_block)
            .event("PairCreated(address,address,address,uint256)")
            .topic2(token);

        let mut logs = match provider.get_logs(&pair_created_filter).await {
            Ok(logs) => logs,
            Err(e) => {
                println!("Error: {}", e);
                max_block_range = max_block_range / 10;
                continue;
            }
        };

        let logs2 = match provider.get_logs(&pair_created_filter_2).await {
            Ok(logs) => logs,
            Err(e) => {
                println!("Error: {}", e);
                max_block_range = max_block_range / 10;
                continue;
            }
        };

        println!("logs1: {}, logs2: {}", logs.len(), logs2.len());

        logs.extend(logs2);

        for log in logs.iter() {
            let token0 = Address::from(log.topics[1]);
            let token1 = Address::from(log.topics[2]);

            let pair_addr_data = &log.data.0[12..32]; // Cut the first 12 bytes
            let pair_addr = Address::from_slice(pair_addr_data);

            results.push((token0, token1, pair_addr));
        }

        current_block = current_to_block + 1;
        max_block_range = max_block_range * 2;
    }
    Ok(results)
}

pub async fn get_v2_weth_pairs(
    provider: &std::sync::Arc<Provider<Ws>>,
    multicall: &mut Multicall<Provider<Ws>>,
    tokens: &[Address],
) -> Result<Vec<Address>> {
    let chain_id = provider.get_chainid().await?.as_u64();
    let v2_factory = UniswapV2Factory::new(get_v2_factory(chain_id)?, provider.clone());
    for token in tokens {
        multicall.add_call(
            v2_factory.get_pair(*token, WETH.parse::<Address>().unwrap()),
            false,
        );
    }

    let weth_pairs_v2 = multicall.call_array().await?;
    multicall.clear_calls();
    Ok(weth_pairs_v2)
}

// Returns (reserve0, reserve1)
pub async fn get_v2_reserves(
    provider: &std::sync::Arc<Provider<Ws>>,
    multicall: &mut Multicall<Provider<Ws>>,
    weth_pairs: &[Address],
) -> Result<Vec<(u128, u128)>> {
    for pair in weth_pairs {
        if pair != &Address::zero() {
            let pair_contract = UniswapV2Pair::new(*pair, provider.clone());
            multicall.add_call(pair_contract.get_reserves(), false);
        }
    }

    let reserves: Vec<(u128, u128, u32)> = multicall.call_array().await?;
    multicall.clear_calls();

    let mut reserve_iter: usize = 0;
    let mut pair_reserves: Vec<(u128, u128)> = Vec::new();

    for i in 0..weth_pairs.len() {
        let pair = weth_pairs[i];

        if pair == Address::zero() {
            pair_reserves.push((0, 0));
            continue;
        }

        pair_reserves.push((reserves[reserve_iter].0, reserves[reserve_iter].1));
        reserve_iter += 1;
    }

    Ok(pair_reserves)
}

pub async fn get_v2_prices_in_weth(
    provider: &std::sync::Arc<Provider<Ws>>,
    multicall: &mut Multicall<Provider<Ws>>,
    reserves: Vec<(u128, u128)>,
    tokens: &[Address],
    amount_weth_in: U256,
) -> Result<Vec<U256>> {
    let weth = WETH.parse::<Address>()?;
    let router = UniswapV2Router02::new(V2_ROUTER.parse::<Address>().unwrap(), provider.clone());

    for (i, reserve) in reserves.clone().into_iter().enumerate() {
        if reserve.0 != 0_u128 {
            let (reserve_in, reserve_out) = if weth.lt(&tokens[i]) {
                (reserve.0, reserve.1)
            } else {
                (reserve.1, reserve.0)
            };
            println!(
                "adding multicall: (weth_in, reserve_in, reserve_out) ({:?}, {:?}, {:?})",
                amount_weth_in, reserve_in, reserve_out
            );
            multicall.add_call(
                router.get_amount_out(
                    amount_weth_in,
                    U256::from(reserve_in),
                    U256::from(reserve_out),
                ),
                false,
            );
        }
    }

    let amounts_out: Vec<U256> = multicall.call_array().await?;
    multicall.clear_calls();

    // account for skipped calls
    let mut prices_per_eth: Vec<U256> = Vec::new();

    let mut skip_iter = 0;
    for reserve in reserves {
        if reserve.0 == 0_u128 {
            prices_per_eth.push(U256::zero());
            continue;
        }

        prices_per_eth.push(amounts_out[skip_iter]);
        skip_iter += 1;
    }

    Ok(prices_per_eth)
}
