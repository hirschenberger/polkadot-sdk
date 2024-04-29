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

use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	traits::{ConstU32, Get},
	BoundedVec, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use scale_info::{
	build::{Fields, Variants},
	Path, Type, TypeInfo,
};
use sp_runtime::{
	traits::{Member, Zero},
	RuntimeDebug,
};
use sp_std::{fmt::Debug, iter::once, ops::Add, prelude::*};

/// An identifier for a single name registrar/identity verification service.
pub type RegistrarIndex = u32;

/// Either underlying data blob if it is at most 32 bytes, or a hash of it. If the data is greater
/// than 32-bytes then it will be truncated when encoding.
///
/// Can also be `None`.
#[derive(Clone, Eq, PartialEq, Debug, MaxEncodedLen)]
pub enum Data {
	/// No data here.
	None,
	/// The data is stored directly.
	Raw(BoundedVec<u8, ConstU32<32>>),
	/// Only the Blake2 hash of the data is stored. The preimage of the hash may be retrieved
	/// through some hash-lookup service.
	BlakeTwo256([u8; 32]),
	/// Only the SHA2-256 hash of the data is stored. The preimage of the hash may be retrieved
	/// through some hash-lookup service.
	Sha256([u8; 32]),
	/// Only the Keccak-256 hash of the data is stored. The preimage of the hash may be retrieved
	/// through some hash-lookup service.
	Keccak256([u8; 32]),
	/// Only the SHA3-256 hash of the data is stored. The preimage of the hash may be retrieved
	/// through some hash-lookup service.
	ShaThree256([u8; 32]),
}

impl Data {
	pub fn is_none(&self) -> bool {
		self == &Data::None
	}
}

impl Decode for Data {
	fn decode<I: codec::Input>(input: &mut I) -> sp_std::result::Result<Self, codec::Error> {
		let b = input.read_byte()?;
		Ok(match b {
			0 => Data::None,
			n @ 1..=33 => {
				let mut r: BoundedVec<_, _> = vec![0u8; n as usize - 1]
					.try_into()
					.expect("bound checked in match arm condition; qed");
				input.read(&mut r[..])?;
				Data::Raw(r)
			},
			34 => Data::BlakeTwo256(<[u8; 32]>::decode(input)?),
			35 => Data::Sha256(<[u8; 32]>::decode(input)?),
			36 => Data::Keccak256(<[u8; 32]>::decode(input)?),
			37 => Data::ShaThree256(<[u8; 32]>::decode(input)?),
			_ => return Err(codec::Error::from("invalid leading byte")),
		})
	}
}

impl Encode for Data {
	fn encode(&self) -> Vec<u8> {
		match self {
			Data::None => vec![0u8; 1],
			Data::Raw(ref x) => {
				let l = x.len().min(32);
				let mut r = vec![l as u8 + 1; l + 1];
				r[1..].copy_from_slice(&x[..l as usize]);
				r
			},
			Data::BlakeTwo256(ref h) => once(34u8).chain(h.iter().cloned()).collect(),
			Data::Sha256(ref h) => once(35u8).chain(h.iter().cloned()).collect(),
			Data::Keccak256(ref h) => once(36u8).chain(h.iter().cloned()).collect(),
			Data::ShaThree256(ref h) => once(37u8).chain(h.iter().cloned()).collect(),
		}
	}
}
impl codec::EncodeLike for Data {}

/// Add a Raw variant with the given index and a fixed sized byte array
macro_rules! data_raw_variants {
    ($variants:ident, $(($index:literal, $size:literal)),* ) => {
		$variants
		$(
			.variant(concat!("Raw", stringify!($size)), |v| v
				.index($index)
				.fields(Fields::unnamed().field(|f| f.ty::<[u8; $size]>()))
			)
		)*
    }
}

impl TypeInfo for Data {
	type Identity = Self;

	fn type_info() -> Type {
		let variants = Variants::new().variant("None", |v| v.index(0));

		// create a variant for all sizes of Raw data from 0-32
		let variants = data_raw_variants!(
			variants,
			(1, 0),
			(2, 1),
			(3, 2),
			(4, 3),
			(5, 4),
			(6, 5),
			(7, 6),
			(8, 7),
			(9, 8),
			(10, 9),
			(11, 10),
			(12, 11),
			(13, 12),
			(14, 13),
			(15, 14),
			(16, 15),
			(17, 16),
			(18, 17),
			(19, 18),
			(20, 19),
			(21, 20),
			(22, 21),
			(23, 22),
			(24, 23),
			(25, 24),
			(26, 25),
			(27, 26),
			(28, 27),
			(29, 28),
			(30, 29),
			(31, 30),
			(32, 31),
			(33, 32)
		);

		let variants = variants
			.variant("BlakeTwo256", |v| {
				v.index(34).fields(Fields::unnamed().field(|f| f.ty::<[u8; 32]>()))
			})
			.variant("Sha256", |v| {
				v.index(35).fields(Fields::unnamed().field(|f| f.ty::<[u8; 32]>()))
			})
			.variant("Keccak256", |v| {
				v.index(36).fields(Fields::unnamed().field(|f| f.ty::<[u8; 32]>()))
			})
			.variant("ShaThree256", |v| {
				v.index(37).fields(Fields::unnamed().field(|f| f.ty::<[u8; 32]>()))
			});

		Type::builder().path(Path::new("Data", module_path!())).variant(variants)
	}
}

impl Default for Data {
	fn default() -> Self {
		Self::None
	}
}

/// An attestation of a registrar over how accurate some `IdentityInfo` is in describing an account.
///
/// NOTE: Registrars may pay little attention to some fields. Registrars may want to make clear
/// which fields their attestation is relevant for by off-chain means.
#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, MaxEncodedLen, TypeInfo)]
pub enum Judgement<Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq>
{
	/// The default value; no opinion is held.
	Unknown,
	/// No judgement is yet in place, but a deposit is reserved as payment for providing one.
	FeePaid(Balance),
	/// The data appears to be reasonably acceptable in terms of its accuracy, however no in depth
	/// checks (such as in-person meetings or formal KYC) have been conducted.
	Reasonable,
	/// The target is known directly by the registrar and the registrar can fully attest to the
	/// the data's accuracy.
	KnownGood,
	/// The data was once good but is currently out of date. There is no malicious intent in the
	/// inaccuracy. This judgement can be removed through updating the data.
	OutOfDate,
	/// The data is imprecise or of sufficiently low-quality to be problematic. It is not
	/// indicative of malicious intent. This judgement can be removed through updating the data.
	LowQuality,
	/// The data is erroneous. This may be indicative of malicious intent. This cannot be removed
	/// except by the registrar.
	Erroneous,
}

impl<Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq>
	Judgement<Balance>
{
	/// Returns `true` if this judgement is indicative of a deposit being currently held. This means
	/// it should not be cleared or replaced except by an operation which utilizes the deposit.
	pub(crate) fn has_deposit(&self) -> bool {
		matches!(self, Judgement::FeePaid(_))
	}

	/// Returns `true` if this judgement is one that should not be generally be replaced outside
	/// of specialized handlers. Examples include "malicious" judgements and deposit-holding
	/// judgements.
	pub(crate) fn is_sticky(&self) -> bool {
		matches!(self, Judgement::FeePaid(_) | Judgement::Erroneous)
	}
}

/// Information concerning the identity of the controller of an account.
pub trait IdentityInformationProvider:
	Encode + Decode + MaxEncodedLen + Clone + Debug + Eq + PartialEq + TypeInfo + Default
{
	/// Type capable of holding information on which identity fields are set.
	type FieldsIdentifier: Member + Encode + Decode + MaxEncodedLen + TypeInfo + Default;

	/// Check if an identity registered information for some given `fields`.
	fn has_identity(&self, fields: Self::FieldsIdentifier) -> bool;

	/// Create a basic instance of the identity information.
	#[cfg(feature = "runtime-benchmarks")]
	fn create_identity_info() -> Self;

	/// The identity information representation for all identity fields enabled.
	#[cfg(feature = "runtime-benchmarks")]
	fn all_fields() -> Self::FieldsIdentifier;
}

/// Information on an identity along with judgements from registrars.
///
/// NOTE: This is stored separately primarily to facilitate the addition of extra fields in a
/// backwards compatible way through a specialized `Decode` impl.
#[derive(CloneNoBound, Encode, Eq, MaxEncodedLen, PartialEqNoBound, Debug, TypeInfo)]
#[codec(mel_bound())]
#[scale_info(skip_type_params(MaxJudgements))]
pub struct Registration<
	Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
	MaxJudgements: Get<u32>,
	IdentityInfo: IdentityInformationProvider,
> {
	/// Judgements from the registrars on this identity. Stored ordered by `RegistrarIndex`. There
	/// may be only a single judgement from each registrar.
	pub judgements: BoundedVec<(RegistrarIndex, Judgement<Balance>), MaxJudgements>,

	/// Amount held on deposit for this information.
	pub deposit: Balance,

	/// Information on the identity.
	pub info: IdentityInfo,
}

impl<
		Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq + Zero + Add,
		MaxJudgements: Get<u32>,
		IdentityInfo: IdentityInformationProvider,
	> Registration<Balance, MaxJudgements, IdentityInfo>
{
	pub(crate) fn total_deposit(&self) -> Balance {
		self.deposit +
			self.judgements
				.iter()
				.map(|(_, ref j)| if let Judgement::FeePaid(fee) = j { *fee } else { Zero::zero() })
				.fold(Zero::zero(), |a, i| a + i)
	}
}

impl<
		Balance: Encode + Decode + MaxEncodedLen + Copy + Clone + Debug + Eq + PartialEq,
		MaxJudgements: Get<u32>,
		IdentityInfo: IdentityInformationProvider,
	> Decode for Registration<Balance, MaxJudgements, IdentityInfo>
{
	fn decode<I: codec::Input>(input: &mut I) -> sp_std::result::Result<Self, codec::Error> {
		let (judgements, deposit, info) = Decode::decode(&mut AppendZerosInput::new(input))?;
		Ok(Self { judgements, deposit, info })
	}
}

/// Information concerning a registrar.
#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, MaxEncodedLen, TypeInfo)]
pub struct RegistrarInfo<
	Balance: Encode + Decode + Clone + Debug + Eq + PartialEq,
	AccountId: Encode + Decode + Clone + Debug + Eq + PartialEq,
	IdField: Encode + Decode + Clone + Debug + Default + Eq + PartialEq + TypeInfo + MaxEncodedLen,
> {
	/// The account of the registrar.
	pub account: AccountId,

	/// Amount required to be given to the registrar for them to provide judgement.
	pub fee: Balance,

	/// Relevant fields for this registrar. Registrar judgements are limited to attestations on
	/// these fields.
	pub fields: IdField,
}

/// Authority properties for a given pallet configuration.
pub type AuthorityPropertiesOf<T> = AuthorityProperties<Suffix<T>>;

/// The number of usernames that an authority may allocate.
type Allocation = u32;
/// A byte vec used to represent a username.
pub(crate) type Suffix<T> = BoundedVec<u8, <T as Config>::MaxSuffixLength>;

/// Properties of a username authority.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Debug)]
pub struct AuthorityProperties<Suffix> {
	/// The suffix added to usernames granted by this authority. Will be appended to usernames; for
	/// example, a suffix of `wallet` will result in `.wallet` being appended to a user's selected
	/// name.
	pub suffix: Suffix,
	/// The number of usernames remaining that this authority can grant.
	pub allocation: Allocation,
}

/// A byte vec used to represent a username.
pub(crate) type Username<T> = BoundedVec<u8, <T as Config>::MaxUsernameLength>;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn manual_data_type_info() {
		let mut registry = scale_info::Registry::new();
		let type_id = registry.register_type(&scale_info::meta_type::<Data>());
		let registry: scale_info::PortableRegistry = registry.into();
		let type_info = registry.resolve(type_id.id).unwrap();

		let check_type_info = |data: &Data| {
			let variant_name = match data {
				Data::None => "None".to_string(),
				Data::BlakeTwo256(_) => "BlakeTwo256".to_string(),
				Data::Sha256(_) => "Sha256".to_string(),
				Data::Keccak256(_) => "Keccak256".to_string(),
				Data::ShaThree256(_) => "ShaThree256".to_string(),
				Data::Raw(bytes) => format!("Raw{}", bytes.len()),
			};
			if let scale_info::TypeDef::Variant(variant) = &type_info.type_def {
				let variant = variant
					.variants
					.iter()
					.find(|v| v.name == variant_name)
					.expect(&format!("Expected to find variant {}", variant_name));

				let field_arr_len = variant
					.fields
					.first()
					.and_then(|f| registry.resolve(f.ty.id))
					.map(|ty| {
						if let scale_info::TypeDef::Array(arr) = &ty.type_def {
							arr.len
						} else {
							panic!("Should be an array type")
						}
					})
					.unwrap_or(0);

				let encoded = data.encode();
				assert_eq!(encoded[0], variant.index);
				assert_eq!(encoded.len() as u32 - 1, field_arr_len);
			} else {
				panic!("Should be a variant type")
			};
		};

		let mut data = vec![
			Data::None,
			Data::BlakeTwo256(Default::default()),
			Data::Sha256(Default::default()),
			Data::Keccak256(Default::default()),
			Data::ShaThree256(Default::default()),
		];

		// A Raw instance for all possible sizes of the Raw data
		for n in 0..32 {
			data.push(Data::Raw(vec![0u8; n as usize].try_into().unwrap()))
		}

		for d in data.iter() {
			check_type_info(d);
		}
	}
}
