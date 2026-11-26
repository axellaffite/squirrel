import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import type { Outcome } from '#lib/files/pages.ts';

export type { Outcome } from '#lib/files/pages.ts';
export type Member = components['schemas']['MemberDto'];
export type Share = components['schemas']['ShareDto'];
export type SharedItem = components['schemas']['SharedItemDto'];
export type SharedByMe = components['schemas']['SharedByMeDto'];
export type Comment = components['schemas']['CommentDto'];
export type CommentPage = components['schemas']['CommentPage'];
export type Permission = components['schemas']['SharePermission'];
export type Access = components['schemas']['AccessLevel'];
export type Friend = components['schemas']['FriendDto'];
export type FriendsPage = components['schemas']['FriendsPage'];

/** What a share or a comment thread is about: one folder or file, or one album. */
export type Subject = { node_id: string } | { album_id: string };

/** Comments fetched per page. */
const COMMENT_PAGE = 50;

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

export const listShares = (subject: Subject): Promise<Outcome<Share[]>> =>
	call(api.GET('/api/shares', { params: { query: subject } }));

/** Shares `subject` with `username`, or changes the permission of an existing share. */
export const shareWith = (subject: Subject, username: string, permission: Permission): Promise<Outcome<Share>> =>
	call(api.POST('/api/shares', { body: { ...subject, username, permission } }));

/** The owner stops sharing, or the recipient leaves. */
export const deleteShare = (id: string): Promise<Outcome<unknown>> =>
	call(api.DELETE('/api/shares/{id}', { params: { path: { id } } }));

export const sharedWithMe = (): Promise<Outcome<SharedItem[]>> => call(api.GET('/api/shared-with-me'));

/** The caller's folders, files and albums shared with someone, with those people. */
export const sharedByMe = (): Promise<Outcome<SharedByMe[]>> => call(api.GET('/api/shared-by-me'));

export const listComments = (subject: Subject, cursor: string | null): Promise<Outcome<CommentPage>> =>
	call(
		api.GET('/api/comments', {
			params: { query: { ...subject, cursor: cursor ?? undefined, limit: COMMENT_PAGE } }
		})
	);

export const postComment = (subject: Subject, body: string): Promise<Outcome<Comment>> =>
	call(api.POST('/api/comments', { body: { ...subject, body } }));

export const editComment = (id: string, body: string): Promise<Outcome<Comment>> =>
	call(api.PATCH('/api/comments/{id}', { params: { path: { id } }, body: { body } }));

export const deleteComment = (id: string): Promise<Outcome<unknown>> =>
	call(api.DELETE('/api/comments/{id}', { params: { path: { id } } }));

/** The caller's friends and the people suggested as friends. */
export const listFriends = (): Promise<Outcome<FriendsPage>> => call(api.GET('/api/friends'));

/** Adds `username` to the caller's friends (already one is fine). */
export const addFriend = (username: string): Promise<Outcome<Friend>> =>
	call(api.POST('/api/friends', { body: { username } }));

export const removeFriend = (username: string): Promise<Outcome<unknown>> =>
	call(api.DELETE('/api/friends/{username}', { params: { path: { username } } }));
