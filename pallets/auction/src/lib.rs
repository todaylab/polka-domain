// This file is part of Polka Domain.

// Copyright (C) 2021 Polka Domain.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{transactional, RuntimeDebug};
use orml_traits::{MultiCurrency, MultiReservableCurrency};
pub use pallet::*;
use primitives::{CurrencyId, NFT};
use sp_runtime::traits::{AtLeast32BitUnsigned, Saturating, Zero};
use sp_std::prelude::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, Default)]
pub struct AuctionDetails<AccountId, Balance, BlockNumber, ClassId, TokenId> {
	creator: AccountId,
	winner: Option<AccountId>,
	token0: (ClassId, TokenId),
	token1: CurrencyId,
	min1: Balance,
	duration: BlockNumber,
	start_at: BlockNumber,
}

pub type MaxAuction = u32;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The units in which we record balances.
		type Balance: Member
			+ Parameter
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize;

		/// The arithmetic type of auction identifier.
		type AuctionId: Member + Parameter + AtLeast32BitUnsigned + Default + Copy;

		/// The currency mechanism.
		type Currency: MultiCurrency<Self::AccountId, CurrencyId = CurrencyId, Balance = Self::Balance>
			+ MultiReservableCurrency<Self::AccountId>;

		/// The class ID type
		type ClassId: Parameter + Member + AtLeast32BitUnsigned + Default + Copy;

		// The token ID type
		type TokenId: Parameter + Member + AtLeast32BitUnsigned + Default + Copy;

		/// The class properties type
		type ClassData: Parameter + Member + MaybeSerializeDeserialize;

		/// The token properties type
		type TokenData: Parameter + Member + MaybeSerializeDeserialize;

		/// The NFT mechanism
		type NFT: NFT<
			Self::AccountId,
			ClassId = Self::ClassId,
			TokenId = Self::TokenId,
			Balance = Self::Balance,
		>;

		/// Max auction allow to create in each block
		type MaxAuction: Get<MaxAuction>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/// Next id of a auction
	#[pallet::storage]
	#[pallet::getter(fn next_auction_id)]
	pub(super) type NextAuctionId<T: Config> = StorageValue<_, T::AuctionId>;

	/// Winner of an auction
	#[pallet::storage]
	#[pallet::getter(fn current_winners)]
	pub(super) type AuctionWinner<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AuctionId, (T::AccountId, T::Balance), ValueQuery>;

	/// Details of an auction.
	#[pallet::storage]
	#[pallet::getter(fn auctions)]
	pub(super) type Auction<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AuctionId,
		AuctionDetails<T::AccountId, T::Balance, T::BlockNumber, T::ClassId, T::TokenId>,
		ValueQuery,
	>;

	/// The end block number of an auction
	#[pallet::storage]
	#[pallet::getter(fn auction_end_at)]
	pub(super) type AuctionEndAt<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::BlockNumber,
		Twox64Concat,
		T::AuctionId,
		Option<()>,
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::metadata(
		T::AccountId = "AccountId",
		T::AuctionId = "AuctionId",
		T::Balance = "Balance",
		T::ClassId = "ClassId",
		T::TokenId = "TokenId",
		CurrencyId = "CurrencyId",
		T::BlockNumber = "BlockNumber"
	)]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AuctionCreated(
			T::AuctionId,
			T::AccountId,
			(T::ClassId, T::TokenId),
			CurrencyId,
			T::Balance,
			T::BlockNumber,
			T::BlockNumber,
			T::BlockNumber,
		),
		AuctionEnd(T::AuctionId, Option<T::AccountId>, Option<T::Balance>),
		AuctionCancelled(T::AuctionId),
		AuctionBid(T::AuctionId, T::AccountId, T::Balance),
	}

	#[pallet::error]
	pub enum Error<T> {
		AuctionExpired,
		AuctionStarted,
		InvalidCreator,
		InvalidDuration,
		InvalidBidAmount,
		ExceedMaxAuction,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(1000)]
		pub fn create_auction(
			origin: OriginFor<T>,
			token0: (T::ClassId, T::TokenId),
			token1: CurrencyId,
			min1: T::Balance,
			duration: T::BlockNumber,
		) -> DispatchResult {
			ensure!(duration > Zero::zero(), Error::<T>::InvalidDuration);

			let creator = ensure_signed(origin)?;
			let auction_id = NextAuctionId::<T>::get().unwrap_or_default();
			let start_at = frame_system::Pallet::<T>::block_number()
				.saturating_add(T::BlockNumber::from(1u32));
			let end_at = start_at.saturating_add(duration);
			ensure!(
				AuctionEndAt::<T>::iter_prefix(end_at).count() <= T::MaxAuction::get() as usize,
				Error::<T>::ExceedMaxAuction
			);

			T::NFT::reserve(&creator, token0)?;

			Auction::<T>::insert(
				auction_id,
				AuctionDetails {
					creator: creator.clone(),
					winner: None,
					token0,
					token1,
					min1,
					duration,
					start_at,
				},
			);
			AuctionEndAt::<T>::insert(end_at, auction_id, Some(()));
			NextAuctionId::<T>::put(auction_id.saturating_add(1u32.into()));

			Self::deposit_event(Event::AuctionCreated(
				auction_id, creator, token0, token1, min1, duration, start_at, end_at,
			));

			Ok(())
		}

		#[pallet::weight(1000)]
		pub fn bid_auction(
			origin: OriginFor<T>,
			auction_id: T::AuctionId,
			amount1: T::Balance,
		) -> DispatchResult {
			let bidder = ensure_signed(origin)?;
			let auction = Auction::<T>::get(auction_id);
			ensure!(amount1 >= auction.min1, Error::<T>::InvalidBidAmount);

			let now = frame_system::Pallet::<T>::block_number();
			ensure!(
				now <= auction.start_at.saturating_add(auction.duration),
				Error::<T>::AuctionExpired
			);

			if AuctionWinner::<T>::contains_key(auction_id) {
				let (maybe_winner, maybe_winner_amount1) = AuctionWinner::<T>::get(auction_id);
				ensure!(amount1 > maybe_winner_amount1, Error::<T>::InvalidBidAmount);
				T::Currency::unreserve(auction.token1, &maybe_winner, maybe_winner_amount1);
			}
			T::Currency::reserve(auction.token1, &bidder, amount1)?;
			AuctionWinner::<T>::insert(auction_id, (&bidder, amount1));

			Self::deposit_event(Event::AuctionBid(auction_id, bidder, amount1));

			Ok(())
		}

		#[pallet::weight(1000)]
		pub fn cancel_auction(origin: OriginFor<T>, auction_id: T::AuctionId) -> DispatchResult {
			let creator = ensure_signed(origin)?;
			let auction = Auction::<T>::get(auction_id);
			ensure!(creator == auction.creator, Error::<T>::InvalidCreator);

			let now = frame_system::Pallet::<T>::block_number();
			ensure!(auction.start_at > now, Error::<T>::AuctionStarted);

			T::NFT::unreserve(&auction.creator, auction.token0)?;
			Auction::<T>::remove(auction_id);

			Self::deposit_event(Event::AuctionCancelled(auction_id));

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(now: T::BlockNumber) {
			Self::on_finalize(now)
				.unwrap_or_else(|_| log::debug!("Unexpected error occurred on finalize."));
		}
	}

	impl<T: Config> Pallet<T> {
		#[transactional]
		fn on_finalize(now: T::BlockNumber) -> DispatchResult {
			for (auction_id, _) in AuctionEndAt::<T>::drain_prefix(&now) {
				Auction::<T>::try_mutate(auction_id, |auction| -> DispatchResult {
					T::NFT::unreserve(&auction.creator, auction.token0)?;

					let mut final_amount = None;
					if AuctionWinner::<T>::contains_key(auction_id) {
						let (winner, winner_amount1) = AuctionWinner::<T>::get(auction_id);
						T::Currency::unreserve(auction.token1, &winner, winner_amount1);
						T::Currency::transfer(
							auction.token1,
							&winner,
							&auction.creator,
							winner_amount1,
						)?;
						T::NFT::transfer(&auction.creator, &winner, auction.token0)?;

						auction.winner = Some(winner);
						final_amount = Some(winner_amount1);
					}

					Self::deposit_event(Event::AuctionEnd(
						auction_id,
						auction.winner.clone(),
						final_amount,
					));
					Ok(())
				})?;
			}
			Ok(())
		}
	}
}
