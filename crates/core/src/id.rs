//! Typed identity, generation, revision and counter newtypes (A-03).
//!
//! Two families with deliberately different semantics:
//!
//! * **Names** ([`ProfileId`], [`SessionId`], [`ExerciseId`], [`WordId`],
//!   [`JobId`]): nonzero `u64` handles allocated by their owning subsystem.
//!   They are stable identities, not generations.
//! * **Counters/generations** ([`EventSeq`], [`CommitSeq`],
//!   [`FrameRevision`], [`SessionEpoch`], [`RequestRevision`],
//!   [`LearnerRevision`], [`LayoutRevision`]): monotonically advancing
//!   `u64` values starting at zero. Advancing is checked; an exhausted
//!   counter returns [`CoreError::IdExhaustion`] instead of wrapping
//!   (AGENTS.md: generations and revisions never wrap).
//!
//! Content versions ([`CorpusVersion`], [`PolicyVersion`]) are
//! content-addressed digests, not ordered numbers: two builds with the same
//! digest carry the same meaning regardless of build order.

use core::fmt;

use crate::CoreError;

macro_rules! typed_name {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(core::num::NonZeroU64);

        impl $name {
            /// Constructs the name from a nonzero `u64`.
            ///
            /// # Errors
            /// Returns [`CoreError::InvalidIdentity`] when `value == 0`.
            pub fn new(value: u64) -> Result<Self, CoreError> {
                core::num::NonZeroU64::new(value)
                    .map(Self)
                    .ok_or(CoreError::InvalidIdentity {
                        kind: stringify!($name),
                        reason: "name identifiers are nonzero",
                    })
            }

            /// The underlying nonzero value.
            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

macro_rules! typed_counter {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(u64);

        impl $name {
            /// The initial value of this counter (zero).
            pub const INITIAL: Self = Self(0);

            /// The underlying value.
            pub const fn get(self) -> u64 {
                self.0
            }

            /// Constructs from a raw value without advancing.
            pub const fn from_raw(value: u64) -> Self {
                Self(value)
            }

            /// Returns the next value after this one.
            ///
            /// # Errors
            /// Returns [`CoreError::IdExhaustion`] rather than wrapping.
            pub fn next(self) -> Result<Self, CoreError> {
                match self.0.checked_add(1) {
                    Some(value) => Ok(Self(value)),
                    None => Err(CoreError::IdExhaustion {
                        kind: stringify!($name),
                        reason: "counter space exhausted; retire the owner instead of wrapping",
                    }),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

typed_name!(
    /// Owner of a learning profile: all sessions, observations and learner
    /// state belong to exactly one profile.
    ProfileId
);
typed_name!(
    /// One guided daily session (or single-track practice session).
    SessionId
);

impl SessionId {
    /// The first legal session identity (compile-time checked nonzero).
    pub const FIRST: Self = Self(core::num::NonZeroU64::MIN);
}
typed_name!(
    /// One immutable target presentation within a session.
    ExerciseId
);
typed_name!(
    /// A corpus word identity, content-addressed within its pack/language
    /// namespace (D-03).
    WordId
);
typed_name!(
    /// One reserved asynchronous job slot (A-04).
    JobId
);
typed_name!(
    /// A prose passage identity derived from its text digest, so it
    /// survives corpus reordering (plan 0003).
    PassageId
);

typed_counter!(
    /// Ordered semantic event sequence within a driver stream or trace
    /// (event sequence zero denotes the initial state, H-03).
    EventSeq
);
typed_counter!(
    /// Monotonic cumulative session checkpoint sequence (A-06).
    CommitSeq
);
typed_counter!(
    /// Revision of a prepared view / rendered frame lineage.
    FrameRevision
);
typed_counter!(
    /// Generation of a session identity; restart/resume advances the epoch
    /// so stale async results are rejected (A-03).
    SessionEpoch
);
typed_counter!(
    /// Revision of a typed request, distinguishing re-issued work from
    /// duplicates (A-03).
    RequestRevision
);
typed_counter!(
    /// Revision of the derived learner model snapshot.
    LearnerRevision
);
typed_counter!(
    /// Generation of an active keyboard layout; changes partition skills
    /// and start a separate profile partition (L-06).
    LayoutRevision
);

/// A 32-byte content digest used for content-addressed versions and commit
/// payloads. Constructed by hashing canonical bytes with SHA-256 (the named
/// hash algorithm/version for candidates and receipts, B-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentDigest([u8; 32]);

impl ContentDigest {
    /// Wraps 32 raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hexadecimal rendering (stable across builds).
    pub fn to_hex(self) -> String {
        let mut out = String::with_capacity(64);
        for byte in self.0 {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Identity of a loaded corpus pack: content digest of its normalized
/// contents plus manifest. Changing corpus content changes this version and
/// invalidates planner inputs without rewriting old exercise targets (D-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CorpusVersion(ContentDigest);

impl CorpusVersion {
    /// Wraps a content digest.
    pub const fn new(digest: ContentDigest) -> Self {
        Self(digest)
    }

    /// The underlying digest.
    pub const fn digest(self) -> ContentDigest {
        self.0
    }
}

/// Identity of a trainer policy document (for example `trainer-v1`):
/// content digest of its validated TOML. Policy changes never rewrite old
/// scores (L-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PolicyVersion(ContentDigest);

impl PolicyVersion {
    /// Wraps a content digest.
    pub const fn new(digest: ContentDigest) -> Self {
        Self(digest)
    }

    /// The underlying digest.
    pub const fn digest(self) -> ContentDigest {
        self.0
    }
}

/// Slot index of a generational view/job slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewSlot(u16);

impl ViewSlot {
    /// Constructs a slot index.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidIdentity`] when `value` is zero; slot
    /// zero is reserved as "no slot" so absence is never a valid index.
    pub fn new(value: u16) -> Result<Self, CoreError> {
        if value == 0 {
            return Err(CoreError::InvalidIdentity {
                kind: "ViewSlot",
                reason: "slot zero is reserved for absence",
            });
        }
        Ok(Self(value))
    }

    /// The underlying slot index (nonzero).
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Identity of an ephemeral generational view (A-03): a slot plus a
/// generation. A stale generation never paints (the absent/deleted owner
/// invalidates pending view work); exhaustion retires the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewId {
    /// Reserved slot index.
    pub slot: ViewSlot,
    /// Generation within the slot.
    pub generation: u64,
}

/// Fixed-point ratio in basis points: an integer `n` in `0..=10_000`
/// representing `n / 10_000`. Candidate utility weights, accuracy targets,
/// pace thresholds and hysteresis bounds are all `BasisPoints` (L-04);
/// there are no naked policy percentages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BasisPoints(u16);

impl BasisPoints {
    /// 100% — the inclusive upper bound.
    pub const MAX: Self = Self(10_000);
    /// 0%.
    pub const ZERO: Self = Self(0);

    /// Constructs basis points from an integer count.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidBasisPoints`] when `value > 10_000`.
    pub const fn new(value: u16) -> Result<Self, CoreError> {
        if value > 10_000 {
            return Err(CoreError::InvalidBasisPoints { value });
        }
        Ok(Self(value))
    }

    /// Constructs from a numerator/denominator pair, truncating toward
    /// zero. Display-target use only; policy comparisons use exact
    /// rational arithmetic at their call sites.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidDenominator`] when `denominator == 0`,
    /// [`CoreError::InvalidBasisPoints`] when the ratio exceeds 100%
    /// (callers must clamp explicitly at a named boundary rather than
    /// silently saturating), or [`CoreError::ArithmeticOverflow`] when the
    /// intermediate multiplication overflows.
    pub fn from_ratio_truncating(numerator: u64, denominator: u64) -> Result<Self, CoreError> {
        match cadence_kernel::ratio::basis_points(numerator, denominator) {
            cadence_kernel::ratio::BasisPoints::ZeroDenominator => {
                Err(CoreError::InvalidDenominator)
            }
            // Ratio exceeds 100%; callers must clamp explicitly at a named
            // boundary rather than silently saturating here.
            cadence_kernel::ratio::BasisPoints::OverFull => {
                Err(CoreError::InvalidBasisPoints { value: u16::MAX })
            }
            cadence_kernel::ratio::BasisPoints::Overflow => Err(CoreError::ArithmeticOverflow {
                operation: "BasisPoints::from_ratio_truncating",
            }),
            cadence_kernel::ratio::BasisPoints::Value(hundredths) => {
                let value = u16::try_from(hundredths)
                    .map_err(|_too_large| CoreError::InvalidBasisPoints { value: u16::MAX })?;
                Self::new(value)
            }
        }
    }

    /// The integer basis-point count.
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Checked addition; two valid ratios can still exceed the range.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidBasisPoints`] when the sum exceeds
    /// 10_000 basis points.
    pub const fn checked_add(self, other: Self) -> Result<Self, CoreError> {
        match self.0.checked_add(other.0) {
            Some(sum) => Self::new(sum),
            None => Err(CoreError::InvalidBasisPoints { value: u16::MAX }),
        }
    }

    /// Checked subtraction; never negative.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidSubtraction`] when `other > self`.
    pub fn checked_sub(self, other: Self) -> Result<Self, CoreError> {
        match self.0.checked_sub(other.0) {
            Some(difference) => Ok(Self(difference)),
            None => Err(CoreError::InvalidSubtraction {
                left: u64::from(self.0),
                right: u64::from(other.0),
            }),
        }
    }
}

impl fmt::Display for BasisPoints {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}bp", self.0)
    }
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.

    use super::*;

    #[test]
    fn names_reject_zero() {
        assert!(ProfileId::new(0).is_err());
        assert!(SessionId::new(1).is_ok());
    }

    #[test]
    fn counters_exhaust_instead_of_wrapping() {
        let exhausted = EventSeq::from_raw(u64::MAX);
        match exhausted.next() {
            Err(CoreError::IdExhaustion { kind, .. }) => assert_eq!(kind, "EventSeq"),
            other => panic!("expected exhaustion, got {other:?}"),
        }
    }

    #[test]
    fn basis_points_bounds() {
        assert!(BasisPoints::new(10_000).is_ok());
        assert!(BasisPoints::new(10_001).is_err());
        assert_eq!(
            BasisPoints::from_ratio_truncating(1, 2).map(BasisPoints::get),
            Ok(5_000)
        );
        assert_eq!(
            BasisPoints::from_ratio_truncating(1, 0),
            Err(CoreError::InvalidDenominator)
        );
        assert_eq!(
            BasisPoints::from_ratio_truncating(2, 1),
            Err(CoreError::InvalidBasisPoints { value: u16::MAX })
        );
        // Scaling a numerator above u64::MAX / 10_000 overflows: refused,
        // never wrapped or clipped to a plausible percentage.
        assert_eq!(
            BasisPoints::from_ratio_truncating(u64::MAX, u64::MAX),
            Err(CoreError::ArithmeticOverflow {
                operation: "BasisPoints::from_ratio_truncating"
            })
        );
        assert_eq!(
            BasisPoints::from_ratio_truncating(2, 3).map(BasisPoints::get),
            Ok(6_666)
        );
    }
}
