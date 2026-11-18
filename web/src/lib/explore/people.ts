/** People and their faces: corrections (rename, hide, merge, split, assign) and face settings. */
import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import type { Face, Person } from '#lib/explore/api.ts';
import { collectPages, type Outcome } from '#lib/files/pages.ts';

export type MlSettings = components['schemas']['MlSettingsDto'];
export type FaceAssigned = components['schemas']['FaceAssigned'];

/** The server's limits. */
export const MAX_MERGE = 50;
export const MAX_SPLIT = 500;
export const MAX_NAME = 100;

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

/** A name to save: trimmed, 1–100 characters, no control character; null when it is not one. */
export function cleanName(raw: string): string | null {
	const name = raw.trim();
	// The server refuses control characters too.
	if (/\p{Cc}/u.test(name)) return null;
	return name.length >= 1 && [...name].length <= MAX_NAME ? name : null;
}

export const renamePerson = (id: string, name: string | null): Promise<Outcome<Person>> =>
	call(
		api.PATCH('/api/people/{id}', {
			params: { path: { id } },
			body: name === null ? { clear_name: true } : { name }
		})
	);

export const setPersonHidden = (id: string, hidden: boolean): Promise<Outcome<Person>> =>
	call(api.PATCH('/api/people/{id}', { params: { path: { id } }, body: { hidden } }));

export const setPersonCover = (id: string, faceId: string | null): Promise<Outcome<Person>> =>
	call(api.PATCH('/api/people/{id}', { params: { path: { id } }, body: { cover_face_id: faceId } }));

/** Moves the faces of `from` to `into` (pinned); `from` are deleted. */
export const mergePeople = (into: string, from: readonly string[]): Promise<Outcome<Person>> =>
	call(api.POST('/api/people/{id}/merge', { params: { path: { id: into } }, body: { from: [...from] } }));

/** Moves these faces of `id` to a new person (pinned). */
export const splitPerson = (id: string, faceIds: readonly string[], name: string | null = null): Promise<Outcome<Person>> =>
	call(api.POST('/api/people/{id}/split', { params: { path: { id } }, body: { face_ids: [...faceIds], name } }));

/** Every face of a person, newest photo first. */
export function listPersonFaces(id: string): Promise<Outcome<Face[]>> {
	return collectPages((cursor) =>
		call(api.GET('/api/people/{id}/faces', { params: { path: { id }, query: { cursor: cursor ?? undefined, limit: 500 } } }))
	);
}

/** Pins a face to a person, or to a new one (optionally named). */
export const assignFace = (faceId: string, to: { person: string } | { newPerson: string | null }): Promise<Outcome<FaceAssigned>> =>
	call(
		api.PUT('/api/faces/{id}/person', {
			params: { path: { id: faceId } },
			body: 'person' in to ? { person_id: to.person } : { new_person: { name: to.newPerson } }
		})
	);

/** "Not this person": pinned to nobody, never grouped again. */
export const unassignFace = (faceId: string): Promise<Outcome<FaceAssigned>> =>
	call(api.DELETE('/api/faces/{id}/person', { params: { path: { id: faceId } } }));

export const getMlSettings = (): Promise<Outcome<MlSettings>> => call(api.GET('/api/me/ml-settings'));

/** Turning faces off deletes every face and person of the caller at once. */
export const setFacesEnabled = (enabled: boolean): Promise<Outcome<MlSettings>> =>
	call(api.PUT('/api/me/ml-settings', { body: { faces_enabled: enabled } }));

/**
 * The person a merge keeps: a named one first (the one with most faces among them), else the
 * one with most faces; ties go to the first chosen.
 */
export function mergeTarget<P extends Pick<Person, 'id' | 'name' | 'face_count'>>(people: readonly P[]): P | null {
	let best: P | null = null;
	for (const person of people) {
		if (!best) {
			best = person;
			continue;
		}
		const named = Boolean(person.name?.trim());
		const bestNamed = Boolean(best.name?.trim());
		if (named !== bestNamed ? named : person.face_count > best.face_count) best = person;
	}
	return best;
}

/** Why `target` is offered as the person a merge keeps. */
export function mergeReason(target: Pick<Person, 'name'>, people: readonly Pick<Person, 'name'>[]): string {
	const named = people.filter((person) => person.name?.trim()).length;
	if (target.name?.trim()) {
		return named > 1 ? 'Chosen because they are named and have the most photos.' : 'Chosen because they are the only one with a name.';
	}
	return 'Chosen because they have the most photos.';
}

/** Why the people grid is empty, worded for the user; null when nothing explains it. */
export function emptyReason(state: {
	facesEnabled: boolean | null;
	facesInstalled: boolean | null;
	analysing: number;
}): { title: string; text: string; settings: boolean } | null {
	if (state.facesEnabled === false) {
		return { title: 'Face recognition is off', text: 'Turn it on in Settings › Faces to group your photos by person.', settings: true };
	}
	if (state.facesInstalled === false) {
		return { title: 'Not set up on this server', text: 'Ask your administrator to install the face models.', settings: false };
	}
	if (state.analysing > 0) {
		const count = new Intl.NumberFormat().format(state.analysing);
		return { title: `Analysing ${count} ${state.analysing === 1 ? 'photo' : 'photos'}…`, text: 'People appear here as faces are found.', settings: false };
	}
	return null;
}
