import { describe, expect, it } from 'vitest';

import {
  accountAge,
  accountDate,
  broadcasterLabel,
  isFounder,
  subscriptionLength,
  subscriptionMonths,
} from './profile';

const now = new Date('2026-09-21T12:00:00Z');

describe('accountAge', () => {
  it('says nothing about an account with no date', () => {
    expect(accountAge(null, now)).toBeNull();
    expect(accountAge('not a date', now)).toBeNull();
  });

  it('counts days, then months, then years', () => {
    expect(accountAge('2026-09-01T12:00:00Z', now)).toBe('20 days');
    expect(accountAge('2026-01-01T12:00:00Z', now)).toBe('8 months');
    expect(accountAge('2019-03-12T12:00:00Z', now)).toBe('7 years');
  });

  /** A clock a few seconds behind Twitch's must not render a negative age. */
  it('reads a brand new account as today', () => {
    expect(accountAge('2026-09-21T12:00:05Z', now)).toBe('today');
  });
});

describe('accountDate', () => {
  it('has nothing to show without a date', () => {
    expect(accountDate(null)).toBeNull();
    expect(accountDate('nonsense')).toBeNull();
  });

  it('renders a real date', () => {
    expect(accountDate('2019-03-12T12:00:00Z')).toBeTruthy();
  });
});

describe('broadcasterLabel', () => {
  it('labels the two Twitch tiers and nothing else', () => {
    expect(broadcasterLabel('partner')).toBe('Partner');
    expect(broadcasterLabel('affiliate')).toBe('Affiliate');
    expect(broadcasterLabel('')).toBeNull();
    expect(broadcasterLabel('staff')).toBeNull();
  });
});

const badge = (setId: string, info = '') => ({ setId, info });

describe('subscriptionMonths', () => {
  it('has nothing to report for a viewer who is not subscribed', () => {
    expect(subscriptionMonths([])).toBeNull();
    expect(subscriptionMonths([badge('moderator', '')])).toBeNull();
  });

  it('reads the count off the subscriber badge', () => {
    expect(subscriptionMonths([badge('moderator'), badge('subscriber', '24')])).toBe(24);
  });

  /** The first month carries the badge with no count behind it. */
  it('counts a subscriber badge with no number as subscribed', () => {
    expect(subscriptionMonths([badge('subscriber', '')])).toBe(0);
  });

  it('reads a founder badge the same way', () => {
    expect(subscriptionMonths([badge('founder', '61')])).toBe(61);
    expect(isFounder([badge('founder', '61')])).toBe(true);
    expect(isFounder([badge('subscriber', '61')])).toBe(false);
  });
});

describe('subscriptionLength', () => {
  it('counts months, then years', () => {
    expect(subscriptionLength(0)).toBe('less than a month');
    expect(subscriptionLength(1)).toBe('1 month');
    expect(subscriptionLength(23)).toBe('23 months');
    expect(subscriptionLength(24)).toBe('2 years');
    expect(subscriptionLength(25)).toBe('2 years, 1 month');
    expect(subscriptionLength(38)).toBe('3 years, 2 months');
  });
});
