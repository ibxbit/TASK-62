"""
Reconciliation discrepancy logic — pure unit tests.

Mirrors the Rust functions in `src/reconciliation/discrepancy.rs`.

Functions under test:
  - find_duplicates   → group statement refs that appear > once
  - canonical_index   → first (minimum) index within a duplicate group
  - is_duplicate      → True if index is in any duplicate group
  - classify_amounts  → Matched | AmountMismatch based on $0.01 tolerance
  - net_discrepancy   → actual − expected (positive = over-collected)
  - DiscrepancySummary.is_high → True if count > 10 or pct > 5%
  - DiscrepancySummary.is_clean → True if no discrepancies
"""

from collections import defaultdict
from dataclasses import dataclass, field

# ── Replicated pure functions ─────────────────────────────────────────────────

AMOUNT_TOLERANCE = 0.01  # matches src/reconciliation/discrepancy.rs


def find_duplicates(records: list[str]) -> dict[str, list[int]]:
    """
    Input: list of reference strings (one per statement row).
    Returns: {ref: [indices]} for references appearing more than once.
    """
    groups: dict[str, list[int]] = defaultdict(list)
    for i, ref in enumerate(records):
        groups[ref].append(i)
    return {ref: idxs for ref, idxs in groups.items() if len(idxs) > 1}


def canonical_index(indices: list[int]) -> int:
    """The canonical entry in a duplicate group is the first occurrence."""
    return min(indices)


def is_duplicate(idx: int, dup_map: dict[str, list[int]]) -> bool:
    return any(idx in idxs for idxs in dup_map.values())


def classify_amounts(expected: float, actual: float) -> str:
    # Use a small epsilon to handle floating point precision
    if abs(expected - actual) <= AMOUNT_TOLERANCE + 1e-9:
        return "matched"
    return "amount_mismatch"


def net_discrepancy(expected: float, actual: float) -> float:
    return actual - expected


@dataclass
class DiscrepancySummary:
    matched: int = 0
    amount_mismatches: int = 0
    missing_from_statement: int = 0
    extra_in_statement: int = 0
    duplicates: int = 0

    def total_discrepancies(self) -> int:
        return (
            self.amount_mismatches
            + self.missing_from_statement
            + self.extra_in_statement
            + self.duplicates
        )

    def is_clean(self) -> bool:
        return self.total_discrepancies() == 0

    def is_high(self, total_records: int) -> bool:
        n = self.total_discrepancies()
        if n > 10:
            return True
        if total_records > 0 and n * 100 // total_records > 5:
            return True
        return False


# ── Tolerance constant ────────────────────────────────────────────────────────

class TestToleranceConstant:
    def test_tolerance_is_one_cent(self):
        assert abs(AMOUNT_TOLERANCE - 0.01) < 1e-10


# ── Amount classification ─────────────────────────────────────────────────────

class TestClassifyAmounts:
    def test_exact_match(self):
        assert classify_amounts(100.00, 100.00) == "matched"

    def test_within_one_cent_over(self):
        assert classify_amounts(100.00, 100.01) == "matched"

    def test_within_one_cent_under(self):
        assert classify_amounts(100.00, 99.99) == "matched"

    def test_exactly_at_tolerance_boundary(self):
        # |100.01 - 100.00| == 0.01 == tolerance → matched
        assert classify_amounts(100.00, 100.01) == "matched"

    def test_two_cents_over_is_mismatch(self):
        assert classify_amounts(100.00, 100.02) == "amount_mismatch"

    def test_two_cents_under_is_mismatch(self):
        assert classify_amounts(100.00, 99.98) == "amount_mismatch"

    def test_large_discrepancy(self):
        assert classify_amounts(500.00, 300.00) == "amount_mismatch"

    def test_zero_vs_positive(self):
        assert classify_amounts(0.00, 100.00) == "amount_mismatch"

    def test_positive_vs_zero(self):
        assert classify_amounts(100.00, 0.00) == "amount_mismatch"


# ── Net discrepancy ───────────────────────────────────────────────────────────

class TestNetDiscrepancy:
    def test_zero_on_exact_match(self):
        assert abs(net_discrepancy(100.0, 100.0)) < 1e-9

    def test_positive_means_over_collected(self):
        d = net_discrepancy(100.0, 120.0)
        assert d > 0
        assert abs(d - 20.0) < 1e-9

    def test_negative_means_under_collected(self):
        d = net_discrepancy(100.0, 80.0)
        assert d < 0
        assert abs(d + 20.0) < 1e-9


# ── Duplicate detection ───────────────────────────────────────────────────────

class TestFindDuplicates:
    def test_no_duplicates_empty_map(self):
        assert find_duplicates(["A", "B", "C"]) == {}

    def test_single_pair(self):
        dups = find_duplicates(["A", "B", "A"])
        assert "A" in dups
        assert dups["A"] == [0, 2]

    def test_multiple_refs_duplicated(self):
        dups = find_duplicates(["X", "Y", "X", "Y"])
        assert "X" in dups and "Y" in dups

    def test_triple_occurrence(self):
        dups = find_duplicates(["Z", "Z", "Z", "A"])
        assert len(dups["Z"]) == 3
        assert "A" not in dups

    def test_empty_input(self):
        assert find_duplicates([]) == {}

    def test_single_element_no_dup(self):
        assert find_duplicates(["ONLY"]) == {}


class TestCanonicalIndex:
    def test_is_minimum(self):
        assert canonical_index([5, 1, 3]) == 1

    def test_single_element(self):
        assert canonical_index([42]) == 42

    def test_already_first(self):
        assert canonical_index([0, 1, 2]) == 0


class TestIsDuplicate:
    def test_identifies_dup_correctly(self):
        dups = find_duplicates(["A", "B", "A"])
        assert is_duplicate(0, dups)
        assert not is_duplicate(1, dups)
        assert is_duplicate(2, dups)


# ── DiscrepancySummary ────────────────────────────────────────────────────────

class TestDiscrepancySummary:
    def test_is_clean_all_matched(self):
        s = DiscrepancySummary(matched=100)
        assert s.is_clean()
        assert s.total_discrepancies() == 0

    def test_total_sums_discrepancy_types(self):
        s = DiscrepancySummary(
            matched=10,
            amount_mismatches=2,
            missing_from_statement=3,
            extra_in_statement=4,
            duplicates=5,
        )
        assert s.total_discrepancies() == 14
        assert not s.is_clean()

    def test_high_by_absolute_count(self):
        s = DiscrepancySummary(amount_mismatches=11)
        assert s.is_high(1000)

    def test_not_high_at_exactly_10(self):
        s = DiscrepancySummary(amount_mismatches=10)
        assert not s.is_high(1000)  # 10 is NOT > 10

    def test_high_by_percentage(self):
        # 6/100 = 6% > 5%
        s = DiscrepancySummary(missing_from_statement=6)
        assert s.is_high(100)

    def test_not_high_at_exactly_5pct(self):
        # 5/100 = 5% → integer division: 5 * 100 // 100 = 5, NOT > 5
        s = DiscrepancySummary(missing_from_statement=5)
        assert not s.is_high(100)

    def test_high_count_wins_even_with_low_pct(self):
        # 11 / 10000 = 0.11% but count 11 > 10 → high
        s = DiscrepancySummary(duplicates=11)
        assert s.is_high(10_000)

    def test_zero_total_records_skips_pct_check(self):
        # 3 is not > 10; 0 total records → pct branch skipped
        s = DiscrepancySummary(amount_mismatches=3)
        assert not s.is_high(0)
