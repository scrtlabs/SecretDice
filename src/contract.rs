use cosmwasm_std::{
    to_binary, Api, BankMsg, Coin, CosmosMsg, Env, Extern, HandleResponse, HandleResult, HumanAddr,
    InitResponse, InitResult, Querier, QueryResult, StdError, StdResult, Storage, Uint128,
};
use cosmwasm_storage::{ReadonlySingleton, Singleton};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaChaRng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize, Clone)]
struct State {
    player_1: Option<HumanAddr>,
    player_1_secret: u128,

    player_2: Option<HumanAddr>,
    player_2_secret: u128,

    dice_result: u8,
    winner: Option<HumanAddr>,
}

impl State {
    pub fn save<S: Storage>(storage: &mut S, data: &State) -> StdResult<()> {
        Singleton::new(storage, b"state").save(data)
    }

    pub fn load<S: Storage>(storage: &S) -> StdResult<State> {
        ReadonlySingleton::new(storage, b"state").load()
    }
}

//////////////////////////////////////////////////////////////////////
//////////////////////////////// Init ////////////////////////////////
//////////////////////////////////////////////////////////////////////

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InitMsg {}

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    _msg: InitMsg,
) -> InitResult {
    let state = State {
        player_1: None,
        player_1_secret: 0,

        player_2: None,
        player_2_secret: 0,

        dice_result: 0,
        winner: None,
    };

    State::save(&mut deps.storage, &state)?;

    Ok(InitResponse::default())
}

//////////////////////////////////////////////////////////////////////
/////////////////////////////// Handle ///////////////////////////////
//////////////////////////////////////////////////////////////////////

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    Join { secret: u128 },
    Leave {},
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> HandleResult {
    match msg {
        HandleMsg::Join { secret } => {
            // player 1 joins, sends a secret and deposits 1 SCRT to the contract
            // player 1's secret is stored privatly
            //
            // player 2 joins, sends a secret and deposits 1 SCRT to the contract
            // player 2's secret is stored privatly
            //
            // once player 2 joins, we can derive a shared secret that no one knows
            // then we can roll the dice and choose a winner
            // dice roll 1-3: player 1 wins / dice roll 4-6: player 2 wins
            //
            // the winner then gets 2 SCRT

            if env.message.sent_funds.len() != 1
                || env.message.sent_funds[0].amount != Uint128(1_000_000 /* 1 SCRT */)
                || env.message.sent_funds[0].denom != String::from("uscrt")
            {
                return Err(StdError::generic_err(
                    "Must deposit 1 SCRT to enter the game.",
                ));
            }

            let mut state = State::load(&deps.storage)?;

            if state.player_1.is_none() {
                state.player_1 = Some(env.message.sender);
                state.player_1_secret = secret;

                State::save(&mut deps.storage, &state)?;

                Ok(HandleResponse::default())
            } else if state.player_2.is_none() {
                state.player_2 = Some(env.message.sender);
                state.player_2_secret = secret;

                let mut combined_secret: Vec<u8> = state.player_1_secret.to_be_bytes().to_vec();
                combined_secret.extend(state.player_2_secret.to_be_bytes().to_vec());

                let random_seed: [u8; 32] = Sha256::digest(&combined_secret).into();
                let mut rng = ChaChaRng::from_seed(random_seed);

                state.dice_result = ((rng.next_u32() % 6) + 1) as u8; // a number between 1 and 6

                if state.dice_result >= 1 && state.dice_result <= 3 {
                    state.winner = state.player_1.clone();
                } else {
                    state.winner = state.player_2.clone();
                }

                State::save(&mut deps.storage, &state.clone())?;

                Ok(HandleResponse {
                    messages: vec![CosmosMsg::Bank(BankMsg::Send {
                        from_address: env.contract.address,
                        to_address: state.winner.unwrap(),
                        amount: vec![Coin::new(2_000_000, "uscrt")],
                    })],
                    log: vec![],
                    data: None,
                })
            } else {
                Err(StdError::generic_err("Game is full."))
            }
        }
        HandleMsg::Leave {} => {
            // if player 2 isn't in yet, player 1 can leave and get their money back

            let mut state = State::load(&deps.storage)?;

            if state.player_1 != Some(env.message.sender.clone()) {
                return Err(StdError::generic_err("You are not a player."));
            }

            if state.winner.is_some() {
                return Err(StdError::generic_err(format!(
                    "Game is already over. Winner is {}.",
                    state.winner.unwrap()
                )));
            }

            state.player_1 = None;
            state.player_1_secret = 0;

            State::save(&mut deps.storage, &state.clone())?;

            Ok(HandleResponse {
                messages: vec![CosmosMsg::Bank(BankMsg::Send {
                    from_address: env.contract.address,
                    to_address: env.message.sender,
                    amount: vec![Coin::new(1_000_000, "uscrt")],
                })],
                log: vec![],
                data: None,
            })
        }
    }
}

///////////////////////////////////////////////////////////////////////
//////////////////////////////// Query ////////////////////////////////
///////////////////////////////////////////////////////////////////////

// These are getters, we only return what's public

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetResult {},
}
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
struct Result {
    winner: HumanAddr,
    dice_roll: u8,
}

pub fn query<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>, msg: QueryMsg) -> QueryResult {
    match msg {
        QueryMsg::GetResult {} => {
            let state = State::load(&deps.storage)?;

            if state.player_1.is_none() || state.player_2.is_none() {
                return Err(StdError::generic_err("Still waiting for players."));
            }

            return Ok(to_binary(&Result {
                winner: state.winner.unwrap(),
                dice_roll: state.dice_result,
            })?);
        }
    }
}
