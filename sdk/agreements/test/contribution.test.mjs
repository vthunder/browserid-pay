import { test } from "node:test";
import assert from "node:assert/strict";
import { totalOf, contributionOf, normalizeContribution } from "../src/agreements.mjs";

const terms = (contribution) => ({
  price: { amount: "100.00", currency: "USD" },
  fees: { custodian: "1.00", adjudicator: "2.00" },
  contribution,
});

test("contribution: max() takes the larger of the absolute and the percentage", () => {
  // 0.01% of $100.00 is 1 cent exactly; the absolute floor of 1 cent ties.
  assert.equal(contributionOf({ fn: "max", amount: "0.01", percentage: 0.01 }, 10000), 1);
  // On a small deal the percentage rounds under a cent, so the absolute wins.
  assert.equal(contributionOf({ fn: "max", amount: "0.01", percentage: 0.01 }, 100), 1);
  // On a large deal the percentage wins.
  assert.equal(contributionOf({ fn: "max", amount: "0.01", percentage: 1 }, 100000), 1000);
});

test("contribution: min() takes the smaller", () => {
  assert.equal(contributionOf({ fn: "min", amount: "0.01", percentage: 1 }, 100000), 1);
});

test("contribution: a single operand needs no fn", () => {
  assert.equal(contributionOf({ percentage: 1 }, 10000), 100);
  assert.equal(contributionOf({ amount: "0.25" }, 10000), 25);
  assert.equal(contributionOf(null, 10000), 0);
});

test("contribution: two operands without fn is refused, not guessed", () => {
  assert.throws(() => contributionOf({ amount: "0.01", percentage: 1 }, 10000), /contribution_fn_required/);
});

test("totalOf includes the combined contribution", () => {
  // price 10000 + fees 300 + max(1, 100) = 10400
  assert.equal(totalOf(terms({ fn: "max", amount: "0.01", percentage: 1 })), 10400);
});

test("normalizeContribution omits absent operands and always carries a percentage", () => {
  assert.deepEqual(normalizeContribution({ recipient: "f@x.test", percentage: 0.5 }), {
    recipient: "f@x.test", percentage: 0.5,
  });
  assert.deepEqual(normalizeContribution({ recipient: "f@x.test", fn: "max", amount: 0.01, percentage: 0.01 }), {
    recipient: "f@x.test", fn: "max", amount: "0.01", percentage: 0.01,
  });
});
