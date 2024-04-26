// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Traits for managing message queuing and handling.

use super::storage::Footprint;
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, Get, TypedGet};
use sp_runtime::{traits::Convert, BoundedSlice, RuntimeDebug};
use sp_std::{fmt::Debug, marker::PhantomData, prelude::*};
use sp_weights::{Weight, WeightMeter};

/// Errors that can happen when attempting to process a message with
/// [`ProcessMessage::process_message()`].
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, TypeInfo, RuntimeDebug)]
pub enum ProcessMessageError {
	/// The message data format is unknown (e.g. unrecognised header)
	BadFormat,
	/// The message data is bad (e.g. decoding returns an error).
	Corrupt,
	/// The message format is unsupported (e.g. old XCM version).
	Unsupported,
	/// Message processing was not attempted because it was not certain that the weight limit
	/// would be respected. The parameter gives the maximum weight which the message could take
	/// to process.
	Overweight(Weight),
	/// The queue wants to give up its current processing slot.
	///
	/// Hints the message processor to cease servicing this queue and proceed to the next
	/// one. This is seen as a *hint*, not an instruction. Implementations must therefore handle
	/// the case that a queue is re-serviced within the same block after *yielding*. A queue is
	/// not required to *yield* again when it is being re-serviced withing the same block.
	Yield,
	/// The message could not be processed for reaching the stack depth limit.
	StackLimitReached,
}

/// Can process messages from a specific origin.
pub trait ProcessMessage {
	/// The transport from where a message originates.
	type Origin: FullCodec + MaxEncodedLen + Clone + Eq + PartialEq + TypeInfo + Debug;

	/// Process the given message, using no more than the remaining `meter` weight to do so.
	///
	/// Returns whether the message was processed.
	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		meter: &mut WeightMeter,
		id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError>;
}

/// Errors that can happen when attempting to execute an overweight message with
/// [`ServiceQueues::execute_overweight()`].
#[derive(Eq, PartialEq, RuntimeDebug)]
pub enum ExecuteOverweightError {
	/// The referenced message was not found.
	NotFound,
	/// The message was already processed.
	///
	/// This can be treated as success condition.
	AlreadyProcessed,
	/// The available weight was insufficient to execute the message.
	InsufficientWeight,
	/// The queue is paused and no message can be executed from it.
	///
	/// This can change at any time and may resolve in the future by re-trying.
	QueuePaused,
	/// An unspecified error.
	Other,
	/// Another call is currently ongoing and prevents this call from executing.
	RecursiveDisallowed,
}

/// Can service queues and execute overweight messages.
pub trait ServiceQueues {
	/// Addresses a specific overweight message.
	type OverweightMessageAddress;

	/// Service all message queues in some fair manner.
	///
	/// - `weight_limit`: The maximum amount of dynamic weight that this call can use.
	///
	/// Returns the dynamic weight used by this call; is never greater than `weight_limit`.
	/// Should only be called in top-level runtime entry points like `on_initialize` or `on_idle`.
	/// Otherwise, stack depth limit errors may be miss-handled.
	fn service_queues(weight_limit: Weight) -> Weight;

	/// Executes a message that could not be executed by [`Self::service_queues()`] because it was
	/// temporarily overweight.
	fn execute_overweight(
		_weight_limit: Weight,
		_address: Self::OverweightMessageAddress,
	) -> Result<Weight, ExecuteOverweightError> {
		Err(ExecuteOverweightError::NotFound)
	}
}

/// Services queues by doing nothing.
pub struct NoopServiceQueues<OverweightAddr>(PhantomData<OverweightAddr>);
impl<OverweightAddr> ServiceQueues for NoopServiceQueues<OverweightAddr> {
	type OverweightMessageAddress = OverweightAddr;

	fn service_queues(_: Weight) -> Weight {
		Weight::zero()
	}
}

/// The resource footprint of a queue.
#[derive(Default, Copy, Clone, Eq, PartialEq, Debug)]
pub struct QueueFootprint {
	/// The number of pages in the queue (including overweight pages).
	pub pages: u32,
	/// The number of pages that are ready (not yet processed and also not overweight).
	pub ready_pages: u32,
	/// The storage footprint of the queue (including overweight messages).
	pub storage: Footprint,
}

/// Can enqueue messages for multiple origins.
pub trait EnqueueMessage<Origin: MaxEncodedLen> {
	/// The maximal length any enqueued message may have.
	type MaxMessageLen: Get<u32>;

	/// Enqueue a single `message` from a specific `origin`.
	fn enqueue_message(message: BoundedSlice<u8, Self::MaxMessageLen>, origin: Origin);

	/// Enqueue multiple `messages` from a specific `origin`.
	fn enqueue_messages<'a>(
		messages: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		origin: Origin,
	);

	/// Any remaining unprocessed messages should happen only lazily, not proactively.
	fn sweep_queue(origin: Origin);

	/// Return the state footprint of the given queue.
	fn footprint(origin: Origin) -> QueueFootprint;
}

impl<Origin: MaxEncodedLen> EnqueueMessage<Origin> for () {
	type MaxMessageLen = ConstU32<0>;
	fn enqueue_message(_: BoundedSlice<u8, Self::MaxMessageLen>, _: Origin) {}
	fn enqueue_messages<'a>(
		_: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		_: Origin,
	) {
	}
	fn sweep_queue(_: Origin) {}
	fn footprint(_: Origin) -> QueueFootprint {
		QueueFootprint::default()
	}
}

/// Transform the origin of an [`EnqueueMessage`] via `C::convert`.
pub struct TransformOrigin<E, O, N, C>(PhantomData<(E, O, N, C)>);
impl<E: EnqueueMessage<O>, O: MaxEncodedLen, N: MaxEncodedLen, C: Convert<N, O>> EnqueueMessage<N>
	for TransformOrigin<E, O, N, C>
{
	type MaxMessageLen = E::MaxMessageLen;

	fn enqueue_message(message: BoundedSlice<u8, Self::MaxMessageLen>, origin: N) {
		E::enqueue_message(message, C::convert(origin));
	}

	fn enqueue_messages<'a>(
		messages: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
		origin: N,
	) {
		E::enqueue_messages(messages, C::convert(origin));
	}

	fn sweep_queue(origin: N) {
		E::sweep_queue(C::convert(origin));
	}

	fn footprint(origin: N) -> QueueFootprint {
		E::footprint(C::convert(origin))
	}
}

/// Handles incoming messages for a single origin.
pub trait HandleMessage {
	/// The maximal length any enqueued message may have.
	type MaxMessageLen: Get<u32>;

	/// Enqueue a single `message` with an implied origin.
	fn handle_message(message: BoundedSlice<u8, Self::MaxMessageLen>);

	/// Enqueue multiple `messages` from an implied origin.
	fn handle_messages<'a>(
		messages: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
	);

	/// Any remaining unprocessed messages should happen only lazily, not proactively.
	fn sweep_queue();

	/// Return the state footprint of the queue.
	fn footprint() -> QueueFootprint;
}

/// Adapter type to transform an [`EnqueueMessage`] with an origin into a [`HandleMessage`] impl.
pub struct EnqueueWithOrigin<E, O>(PhantomData<(E, O)>);
impl<E: EnqueueMessage<O::Type>, O: TypedGet> HandleMessage for EnqueueWithOrigin<E, O>
where
	O::Type: MaxEncodedLen,
{
	type MaxMessageLen = E::MaxMessageLen;

	fn handle_message(message: BoundedSlice<u8, Self::MaxMessageLen>) {
		E::enqueue_message(message, O::get());
	}

	fn handle_messages<'a>(
		messages: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>,
	) {
		E::enqueue_messages(messages, O::get());
	}

	fn sweep_queue() {
		E::sweep_queue(O::get());
	}

	fn footprint() -> QueueFootprint {
		E::footprint(O::get())
	}
}

/// Provides information on paused queues.
pub trait QueuePausedQuery<Origin> {
	/// Whether this queue is paused.
	fn is_paused(origin: &Origin) -> bool;
}

#[impl_trait_for_tuples::impl_for_tuples(8)]
impl<Origin> QueuePausedQuery<Origin> for Tuple {
	fn is_paused(origin: &Origin) -> bool {
		for_tuples!( #(
			if Tuple::is_paused(origin) {
				return true;
			}
		)* );
		false
	}
}
