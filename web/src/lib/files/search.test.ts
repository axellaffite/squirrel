import { describe, expect, it } from 'vitest';
import {
	isSearchable,
	kindLabel,
	nextLimit,
	normalizeQuery,
	pagesLabel,
	shortWordHint,
	retryDelay,
	searchError,
	snippetParts
} from '#lib/files/search.ts';

describe('queries', () => {
	it('normalises and checks length', () => {
		expect(normalizeQuery('  école   élec ')).toBe('école élec');
		expect(isSearchable('a')).toBe(false);
		expect(isSearchable(' ab ')).toBe(true);
		expect(isSearchable('x'.repeat(201))).toBe(false);
	});

	it('explains how short words are matched', () => {
		expect(shortWordHint('le bail')).toBe('Two-letter words only match whole words.');
		expect(shortWordHint('a bail')).toBe('One-letter words are ignored.');
		expect(shortWordHint('a le bail')).toBe('One-letter words are ignored, and two-letter words only match whole words.');
		expect(shortWordHint('bail 2024')).toBeNull();
		expect(shortWordHint('')).toBeNull();
	});

	it('never asks past the 200th result', () => {
		expect(nextLimit(0)).toBe(20);
		expect(nextLimit(190)).toBe(10);
		expect(nextLimit(200)).toBe(0);
	});
});

describe('snippetParts', () => {
	it('merges neighbours of the same kind', () => {
		expect(
			snippetParts([
				{ text: 'the ', highlight: false },
				{ text: 'lease', highlight: true },
				{ text: ' of ', highlight: false },
				{ text: 'the house', highlight: false }
			])
		).toEqual([
			{ text: 'the ', highlight: false },
			{ text: 'lease', highlight: true },
			{ text: ' of the house', highlight: false }
		]);
	});

	it('cuts the snippet at the cap', () => {
		const parts = snippetParts(
			[
				{ text: 'abcdef', highlight: false },
				{ text: 'ghij', highlight: true },
				{ text: 'never shown', highlight: false }
			],
			8
		);
		expect(parts).toEqual([
			{ text: 'abcdef', highlight: false },
			{ text: 'gh…', highlight: true }
		]);
	});

	it('keeps markup as plain text', () => {
		expect(snippetParts([{ text: '<b>x</b>', highlight: true }])).toEqual([{ text: '<b>x</b>', highlight: true }]);
	});
});

describe('wording', () => {
	it('explains rate limits and timeouts calmly', () => {
		expect(searchError(429, 'x')).toBe('Searching a little too fast. Trying again in a moment…');
		expect(searchError(503, 'x')).toMatch(/took too long/);
		expect(searchError(422, 'x')).toBe('Searches use 2 to 200 characters.');
		expect(searchError(500, 'fallback')).toBe('fallback');
	});

	it('labels kinds and pages', () => {
		expect(kindLabel('application/pdf')).toBe('PDF');
		expect(kindLabel('image/jpeg')).toBe('JPEG image');
		expect(kindLabel('application/zip')).toBeNull();
		expect(pagesLabel(3)).toBe('3 pages');
		expect(pagesLabel(1)).toBe('1 page');
		expect(pagesLabel(null)).toBeNull();
	});
});

describe('retryDelay', () => {
	it('reads Retry-After in seconds, else waits a second', () => {
		expect(retryDelay('2')).toBe(2000);
		expect(retryDelay(null)).toBe(1000);
		expect(retryDelay('soon')).toBe(1000);
	});
});
