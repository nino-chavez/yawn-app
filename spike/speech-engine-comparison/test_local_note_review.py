import json
import unittest

from review_notes_local import grounded_flags


class LocalReviewTests(unittest.TestCase):
    def setUp(self):
        self.turns = [{"text": "I will send the report Friday.", "start": 3.0}]
        self.claims = [{"claim_ordinal": 0, "claim": "The report is due Thursday."}]
        self.flag = {"kind": "possible_inaccuracy", "claim_ordinal": 0, "turn": 0,
                     "quote": "Friday", "concern": "Check the deadline."}

    def check(self, flag):
        return grounded_flags(json.dumps({"flags": [flag]}), self.turns, self.claims)

    def test_exact_source_quote_becomes_pending_suggestion(self):
        result = self.check(self.flag)
        self.assertEqual(result["human_review"], "pending")
        flag = result["flags"][0]
        self.assertEqual(self.turns[0]["text"][flag["char_start"]:flag["char_end"]], "Friday")

    def test_invented_quote_turn_and_claim_are_rejected(self):
        for edit in ({"quote": "Thursday"}, {"turn": 1}, {"turn": True}, {"claim_ordinal": 4}, {"kind": "approved"}):
            with self.subTest(edit=edit):
                result = self.check(self.flag | edit)
                self.assertEqual(result["flags"], [])
                self.assertEqual(result["rejected_flags"], 1)

    def test_omission_has_no_invented_claim_identity(self):
        result = self.check(self.flag | {"kind": "possible_omission", "claim_ordinal": None})
        self.assertEqual(len(result["flags"]), 1)

    def test_empty_flags_do_not_approve(self):
        self.assertEqual(grounded_flags('{"flags": []}', self.turns, self.claims)["human_review"], "pending")

    def test_malformed_or_oversized_response_refuses(self):
        for raw in ('not JSON', '{"approved":true}', json.dumps({"flags": [self.flag] * 7})):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                grounded_flags(raw, self.turns, self.claims)


if __name__ == "__main__":
    unittest.main()
