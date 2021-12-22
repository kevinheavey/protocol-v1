use crate::error::*;
use crate::math_error;
use crate::state::user_orders::OrderDiscountTier;
use crate::state::state::{DiscountTokenTier, FeeStructure};
use crate::state::user::User;
use anchor_lang::Account;
use solana_program::msg;
use spl_token::state::Account as TokenAccount;
use crate::state::order_state::OrderFillerRewardStructure;

pub fn calculate_fee_for_market_order(
    quote_asset_amount: u128,
    fee_structure: &FeeStructure,
    discount_token: Option<TokenAccount>,
    referrer: &Option<Account<User>>,
) -> ClearingHouseResult<(u128, u128, u128, u128, u128)> {
    let fee = quote_asset_amount
        .checked_mul(fee_structure.fee_numerator)
        .ok_or_else(math_error!())?
        .checked_div(fee_structure.fee_denominator)
        .ok_or_else(math_error!())?;

    let token_discount = calculate_token_discount(fee, fee_structure, discount_token);

    let (referrer_reward, referee_discount) =
        calculate_referral_reward_and_referee_discount(fee, fee_structure, referrer)?;

    let user_fee = fee
        .checked_sub(token_discount)
        .ok_or_else(math_error!())?
        .checked_sub(referee_discount)
        .ok_or_else(math_error!())?;

    let fee_to_market = user_fee
        .checked_sub(referrer_reward)
        .ok_or_else(math_error!())?;

    return Ok((
        user_fee,
        fee_to_market,
        token_discount,
        referrer_reward,
        referee_discount,
    ));
}

fn calculate_token_discount(
    fee: u128,
    fee_structure: &FeeStructure,
    discount_token: Option<TokenAccount>,
) -> u128 {
    if discount_token.is_none() {
        return 0;
    }

    let discount_token = discount_token.unwrap();

    if let Some(discount) = try_to_calculate_token_discount_for_tier(
        fee,
        &fee_structure.discount_token_tiers.first_tier,
        discount_token,
    ) {
        return discount;
    }

    if let Some(discount) = try_to_calculate_token_discount_for_tier(
        fee,
        &fee_structure.discount_token_tiers.second_tier,
        discount_token,
    ) {
        return discount;
    }

    if let Some(discount) = try_to_calculate_token_discount_for_tier(
        fee,
        &fee_structure.discount_token_tiers.third_tier,
        discount_token,
    ) {
        return discount;
    }

    if let Some(discount) = try_to_calculate_token_discount_for_tier(
        fee,
        &fee_structure.discount_token_tiers.fourth_tier,
        discount_token,
    ) {
        return discount;
    }

    return 0;
}

fn try_to_calculate_token_discount_for_tier(
    fee: u128,
    tier: &DiscountTokenTier,
    discount_token: TokenAccount,
) -> Option<u128> {
    if belongs_to_tier(tier, discount_token) {
        return calculate_token_discount_for_tier(fee, tier);
    }
    return None;
}

fn calculate_token_discount_for_tier(fee: u128, tier: &DiscountTokenTier) -> Option<u128> {
    return Some(
        fee.checked_mul(tier.discount_numerator)?
            .checked_div(tier.discount_denominator)?,
    );
}

fn belongs_to_tier(tier: &DiscountTokenTier, discount_token: TokenAccount) -> bool {
    return discount_token.amount >= tier.minimum_balance;
}

fn calculate_referral_reward_and_referee_discount(
    fee: u128,
    fee_structure: &FeeStructure,
    referrer: &Option<Account<User>>,
) -> ClearingHouseResult<(u128, u128)> {
    if referrer.is_none() {
        return Ok((0, 0));
    }

    let referrer_reward = fee
        .checked_mul(fee_structure.referral_discount.referrer_reward_numerator)
        .ok_or_else(math_error!())?
        .checked_div(fee_structure.referral_discount.referrer_reward_denominator)
        .ok_or_else(math_error!())?;

    let referee_discount = fee
        .checked_mul(fee_structure.referral_discount.referee_discount_numerator)
        .ok_or_else(math_error!())?
        .checked_div(fee_structure.referral_discount.referee_discount_denominator)
        .ok_or_else(math_error!())?;

    return Ok((referrer_reward, referee_discount));
}

pub fn calculate_order_fee_tier(
    fee_structure: &FeeStructure,
    discount_token: Option<TokenAccount>,
) -> ClearingHouseResult<OrderDiscountTier> {
    if discount_token.is_none() {
        return Ok(OrderDiscountTier::None);
    }

    let discount_token = discount_token.unwrap();

    if belongs_to_tier(
        &fee_structure.discount_token_tiers.first_tier,
        discount_token,
    ) {
        return Ok(OrderDiscountTier::First);
    }

    if belongs_to_tier(
        &fee_structure.discount_token_tiers.second_tier,
        discount_token,
    ) {
        return Ok(OrderDiscountTier::Second);
    }

    if belongs_to_tier(
        &fee_structure.discount_token_tiers.third_tier,
        discount_token,
    ) {
        return Ok(OrderDiscountTier::Third);
    }

    if belongs_to_tier(
        &fee_structure.discount_token_tiers.fourth_tier,
        discount_token,
    ) {
        return Ok(OrderDiscountTier::Fourth);
    }

    return Ok(OrderDiscountTier::None);
}

pub fn calculate_fee_for_limit_order(
    quote_asset_amount: u128,
    fee_structure: &FeeStructure,
    filler_reward_structure: &OrderFillerRewardStructure,
    order_fee_tier: &OrderDiscountTier,
) -> ClearingHouseResult<(u128, u128, u128, u128)> {
    let fee = quote_asset_amount
        .checked_mul(fee_structure.fee_numerator)
        .ok_or_else(math_error!())?
        .checked_div(fee_structure.fee_denominator)
        .ok_or_else(math_error!())?;

    let token_discount = calculate_token_discount_for_limit_order(fee, fee_structure, order_fee_tier)?;

    let user_fee = fee
        .checked_sub(token_discount)
        .ok_or_else(math_error!())?;

    let filler_reward = calculate_filler_reward(user_fee, filler_reward_structure)?;
    let fee_to_market = user_fee
        .checked_sub(filler_reward)
        .ok_or_else(math_error!())?;

    return Ok((user_fee, fee_to_market, token_discount, filler_reward));
}

fn calculate_token_discount_for_limit_order(
    fee: u128,
    fee_structure: &FeeStructure,
    order_discount_tier: &OrderDiscountTier,
) -> ClearingHouseResult<u128> {
    return match order_discount_tier {
        OrderDiscountTier::None => Ok(0),
        OrderDiscountTier::First => {
            calculate_token_discount_for_tier(fee, &fee_structure.discount_token_tiers.first_tier)
                .ok_or_else(math_error!())
        },
        OrderDiscountTier::Second => {
            calculate_token_discount_for_tier(fee, &fee_structure.discount_token_tiers.second_tier)
                .ok_or_else(math_error!())
        },
        OrderDiscountTier::Third => {
            calculate_token_discount_for_tier(fee, &fee_structure.discount_token_tiers.third_tier)
                .ok_or_else(math_error!())
        },
        OrderDiscountTier::Fourth => {
            calculate_token_discount_for_tier(fee, &fee_structure.discount_token_tiers.fourth_tier)
                .ok_or_else(math_error!())
        }
    };
}

fn calculate_filler_reward(fee: u128, filler_reward_structure: &OrderFillerRewardStructure) -> ClearingHouseResult<u128> {
    return fee.checked_mul(filler_reward_structure.reward_numerator)
        .ok_or_else(math_error!())?
        .checked_div(filler_reward_structure.reward_denominator)
        .ok_or_else(math_error!());
}
