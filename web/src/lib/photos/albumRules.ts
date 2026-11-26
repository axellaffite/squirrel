/**
 * A dynamic album's rules as the editor holds them, the request body they become, and their
 * wording. Mirrors the server: every group given must hold (AND), an empty one does not
 * constrain, at least one condition, at most 20 people and 20 tags, a region or city needs a
 * country, a camera model needs its make, dates are inclusive `YYYY-MM-DD` days.
 */
import { countryName, tagLabel, type PhotoKind } from '#lib/explore/filters.ts';
import type { AlbumRules, MatchMode } from '#lib/photos/api.ts';

/** The server's limit on people and on tags in one album's rules. */
export const MAX_RULE_ITEMS = 20;

/** How long the editor waits after a change before asking how many photos match. */
export const PREVIEW_DELAY_MS = 300;

/** How many of the newest matching photos the editor shows. */
export const PREVIEW_THUMBS = 6;

export interface RulePlace {
	country: string;
	/** Empty: places without a region. */
	region?: string;
	city?: string;
}

export interface RuleCamera {
	make: string;
	model?: string;
}

export type RuleKind = 'any' | PhotoKind;

export interface RulesForm {
	people: readonly string[];
	peopleMatch: MatchMode;
	tags: readonly string[];
	tagsMatch: MatchMode;
	place: RulePlace | null;
	camera: RuleCamera | null;
	kind: RuleKind;
	/** `YYYY-MM-DD`, or empty for no bound. */
	takenFrom: string;
	takenTo: string;
}

export const emptyRulesForm = (): RulesForm => ({
	people: [],
	peopleMatch: 'any',
	tags: [],
	tagsMatch: 'any',
	place: null,
	camera: null,
	kind: 'any',
	takenFrom: '',
	takenTo: ''
});

const present = (value: string | null | undefined): value is string => typeof value === 'string' && value !== '';

function placeOf(rules: AlbumRules): RulePlace | null {
	if (!present(rules.country)) return null;
	const place: RulePlace = { country: rules.country };
	if (typeof rules.region === 'string') place.region = rules.region;
	if (present(rules.city)) place.city = rules.city;
	return place;
}

function cameraOf(rules: AlbumRules): RuleCamera | null {
	if (!present(rules.camera_make)) return null;
	return present(rules.camera_model) ? { make: rules.camera_make, model: rules.camera_model } : { make: rules.camera_make };
}

/** The editor's state for rules from the server (none: an empty form). */
export function formFromRules(rules: AlbumRules | null | undefined): RulesForm {
	if (!rules) return emptyRulesForm();
	return {
		people: [...(rules.people ?? [])],
		peopleMatch: rules.people_match ?? 'any',
		tags: [...(rules.tags ?? [])],
		tagsMatch: rules.tags_match ?? 'any',
		place: placeOf(rules),
		camera: cameraOf(rules),
		kind: rules.kind ?? 'any',
		takenFrom: rules.taken_from ?? '',
		takenTo: rules.taken_to ?? ''
	};
}

/** The request body: only the conditions that constrain. */
export function rulesBody(form: RulesForm): AlbumRules {
	const body: AlbumRules = {
		people: [...form.people],
		people_match: form.peopleMatch,
		tags: [...form.tags],
		tags_match: form.tagsMatch
	};
	if (form.place) {
		body.country = form.place.country;
		if (form.place.region !== undefined) body.region = form.place.region;
		if (form.place.city) body.city = form.place.city;
	}
	if (form.camera) {
		body.camera_make = form.camera.make;
		if (form.camera.model) body.camera_model = form.camera.model;
	}
	if (form.kind !== 'any') body.kind = form.kind;
	if (form.takenFrom) body.taken_from = form.takenFrom;
	if (form.takenTo) body.taken_to = form.takenTo;
	return body;
}

export const hasCondition = (form: RulesForm): boolean =>
	form.people.length > 0 ||
	form.tags.length > 0 ||
	form.place !== null ||
	form.camera !== null ||
	form.kind !== 'any' ||
	form.takenFrom !== '' ||
	form.takenTo !== '';

/** A real calendar day written `YYYY-MM-DD`. */
function isDay(value: string): boolean {
	if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
	const date = new Date(`${value}T00:00:00Z`);
	return !Number.isNaN(date.getTime()) && date.toISOString().startsWith(value);
}

/** Why the server would refuse these rules, or null. */
export function rulesProblem(form: RulesForm): string | null {
	if (!hasCondition(form)) return 'Choose at least one condition.';
	if (form.people.length > MAX_RULE_ITEMS) return `Choose at most ${MAX_RULE_ITEMS} people.`;
	if (form.tags.length > MAX_RULE_ITEMS) return `Choose at most ${MAX_RULE_ITEMS} tags.`;
	if (form.takenFrom && !isDay(form.takenFrom)) return 'Enter the start date as a real day.';
	if (form.takenTo && !isDay(form.takenTo)) return 'Enter the end date as a real day.';
	if (form.takenFrom && form.takenTo && form.takenTo < form.takenFrom) return 'The end date is before the start date.';
	return null;
}

/** `list` with `item` at the end (unchanged when already there or full). */
export function withItem(list: readonly string[], item: string): readonly string[] {
	if (list.includes(item) || list.length >= MAX_RULE_ITEMS) return list;
	return [...list, item];
}

export const withoutItem = (list: readonly string[], item: string): string[] => list.filter((entry) => entry !== item);

/** "Alice", "Alice and Bob", "Alice, Bob or Carol". */
function joined(names: readonly string[], match: MatchMode): string {
	if (names.length <= 1) return names.join('');
	const word = match === 'all' ? 'and' : 'or';
	return `${names.slice(0, -1).join(', ')} ${word} ${names.at(-1)}`;
}

export function placeLabel(place: RulePlace, locale?: string): string {
	return place.city || place.region || countryName(place.country, locale);
}

export const cameraLabel = (camera: RuleCamera): string => (camera.model ? `${camera.make} ${camera.model}` : camera.make);

export const KIND_LABELS: Record<PhotoKind, string> = { photo: 'Photos only', screenshot: 'Screenshots' };

function dayLabel(day: string, locale?: string): string {
	const format = new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeZone: 'UTC' });
	return isDay(day) ? format.format(new Date(`${day}T00:00:00Z`)) : day;
}

function rangeLabel(from: string, to: string, locale?: string): string | null {
	if (from && to) return `${dayLabel(from, locale)} – ${dayLabel(to, locale)}`;
	if (from) return `Since ${dayLabel(from, locale)}`;
	if (to) return `Until ${dayLabel(to, locale)}`;
	return null;
}

/** One short line per condition, in editor order ("Alice and Bob", "Beach", "Paris", …). */
export function ruleParts(form: RulesForm, personName: (id: string) => string | null, locale?: string): string[] {
	const parts: (string | null)[] = [
		form.people.length > 0 ? joined(form.people.map((id) => personName(id) ?? 'Unnamed'), form.peopleMatch) : null,
		form.tags.length > 0 ? joined(form.tags.map(tagLabel), form.tagsMatch) : null,
		form.place ? placeLabel(form.place, locale) : null,
		form.camera ? cameraLabel(form.camera) : null,
		form.kind !== 'any' ? KIND_LABELS[form.kind] : null,
		rangeLabel(form.takenFrom, form.takenTo, locale)
	];
	return parts.filter((part): part is string => part !== null);
}
