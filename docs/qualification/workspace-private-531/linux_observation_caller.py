"""Retired quota-specific production entry; retained imports fail before effects.

Task 829 replaces it with the single physically bounded native preparation and
session path. Historical quota parsers remain available for old evidence/tests.
"""
from linux_supervisor import Refusal


def execute(facts, stage_root, budget_seconds, clock=None, wall_clock=None):
    raise Refusal('legacy quota preparation entry retired; use the physical native preparation path')
