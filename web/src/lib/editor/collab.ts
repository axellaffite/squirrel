/**
 * One live editing session: a Yjs document synced over y-websocket and bound to CodeMirror.
 * Imported dynamically by the editor page only, so the editor code stays out of the main bundle.
 */
import { defaultKeymap, indentWithTab } from '@codemirror/commands';
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import { yCollab, ySyncAnnotation, yUndoManagerKeymap } from 'y-codemirror.next';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';
import { collabServerUrl, type CollabTicket } from '#lib/editor/api.ts';
import {
	closeOutcome,
	presence,
	sanitizeUser,
	shouldReconnect,
	type CloseOutcome,
	type Person,
	type Phase,
	type PresenceUser
} from '#lib/editor/logic.ts';

/** y-websocket's room segment: the server route is `/api/files/{id}/collab/ws`. */
const ROOM = 'ws';
/** The text of the document, as the server names it. */
const TEXT_NAME = 'content';
/** Upper bound of reconnection back-off. */
const MAX_BACKOFF_MS = 10_000;

export interface CollabOptions {
	parent: HTMLElement;
	nodeId: string;
	/** For the editor's accessible name. */
	fileName: string;
	ticket: CollabTicket;
	user: PresenceUser;
	onPhase: (phase: Phase) => void;
	onPeople: (people: Person[]) => void;
	/** The document changed after the first sync (locally or remotely). */
	onChange: () => void;
	/** The document is in sync with the server (after each connection). */
	onSynced: () => void;
	/** A local edit was refused because the document would exceed `ticket.max_bytes`. */
	onLimit: () => void;
	/** The server closed for good (`end`) or asked for a fresh document (`reopen`). */
	onClosed: (outcome: Exclude<CloseOutcome, { kind: 'reconnect' }>) => void;
}

export interface CollabSession {
	destroy: () => void;
	focus: () => void;
}

const theme = EditorView.theme({
	'&': {
		height: '100%',
		color: 'var(--ink)',
		backgroundColor: 'var(--card)',
		fontSize: 'var(--text-md)'
	},
	'&.cm-focused': { outline: 'none' },
	'.cm-scroller': {
		fontFamily: 'var(--font-mono)',
		lineHeight: 'var(--text-md-lh)'
	},
	'.cm-content': {
		padding: 'var(--space-4) 0',
		caretColor: 'var(--accent)'
	},
	'.cm-line': { padding: '0 var(--space-4)' },
	'.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--accent)' },
	'.cm-content ::selection': { backgroundColor: 'var(--accent-soft)' },
	'.cm-ySelectionInfo': {
		color: 'var(--card)',
		fontFamily: 'var(--font)',
		fontSize: 'var(--text-xs)',
		lineHeight: 'var(--text-xs-lh)',
		fontWeight: 'var(--weight-label)',
		padding: '0 var(--space-1)',
		borderRadius: 'var(--radius-full)'
	}
});

const encoder = new TextEncoder();

/** Refuses local edits that would push the text above `maxBytes` (remote updates always apply). */
function sizeLimit(maxBytes: number, onLimit: () => void): Extension {
	return EditorState.transactionFilter.of((tr) => {
		if (!tr.docChanged || tr.annotation(ySyncAnnotation) !== undefined) return tr;
		// UTF-8 takes at most 3 bytes per UTF-16 unit: only measure when the bound could be crossed.
		if (tr.newDoc.length * 3 <= maxBytes) return tr;
		if (tr.newDoc.length <= tr.startState.doc.length) return tr;
		if (encoder.encode(tr.newDoc.toString()).length <= maxBytes) return tr;
		onLimit();
		return [];
	});
}

/** Remote peers publish their own `user`; keep only known colours and a bounded name before rendering. */
function sanitizeRemoteUsers(awareness: WebsocketProvider['awareness'], selfId: number): void {
	awareness.on('change', ({ added, updated }: { added: number[]; updated: number[] }) => {
		const states = awareness.getStates();
		for (const clientId of [...added, ...updated]) {
			if (clientId === selfId) continue;
			const state = states.get(clientId);
			if (!state || !('user' in state)) continue;
			const user = sanitizeUser(state.user);
			if (JSON.stringify(user) !== JSON.stringify(state.user)) states.set(clientId, { ...state, user });
		}
	});
}

export function startCollab(options: CollabOptions): CollabSession {
	const { ticket } = options;
	const doc = new Y.Doc();
	const text = doc.getText(TEXT_NAME);
	const provider = new WebsocketProvider(collabServerUrl(options.nodeId, window.location), ROOM, doc, {
		params: { epoch: ticket.epoch },
		// The server is the only path between tabs: it enforces access and the epoch.
		disableBc: true,
		maxBackoffTime: MAX_BACKOFF_MS,
		shouldReconnect: (event) => shouldReconnect(event.code)
	});
	const { awareness } = provider;
	let ended = false;
	let connectedOnce = false;
	let synced = false;

	sanitizeRemoteUsers(awareness, doc.clientID);
	awareness.setLocalStateField('user', options.user);
	const publishPeople = () => options.onPeople(presence(awareness.getStates(), doc.clientID));
	awareness.on('change', publishPeople);
	publishPeople();

	provider.on('status', ({ status }) => {
		if (ended) return;
		if (status === 'connected') connectedOnce = true;
		options.onPhase(status === 'connected' ? 'connected' : connectedOnce ? 'reconnecting' : 'connecting');
	});
	provider.on('sync', (isSynced: boolean) => {
		if (isSynced) {
			synced = true;
			options.onSynced();
		}
	});
	provider.on('closed', ({ code }) => {
		const outcome = closeOutcome(code);
		if (outcome.kind === 'reconnect') return;
		ended = true;
		options.onPhase('ended');
		options.onClosed(outcome);
	});
	doc.on('update', () => {
		if (synced) options.onChange();
	});

	const extensions: Extension[] = [
		theme,
		EditorView.lineWrapping,
		EditorView.contentAttributes.of({ 'aria-label': `Contents of ${options.fileName}` }),
		yCollab(text, awareness, { undoManager: ticket.can_write ? new Y.UndoManager(text) : false })
	];
	if (ticket.can_write) {
		extensions.push(keymap.of([...yUndoManagerKeymap, ...defaultKeymap, indentWithTab]), sizeLimit(ticket.max_bytes, options.onLimit));
	} else {
		extensions.push(keymap.of(defaultKeymap), EditorState.readOnly.of(true), EditorView.editable.of(false));
	}

	const view = new EditorView({
		parent: options.parent,
		state: EditorState.create({ doc: text.toString(), extensions })
	});

	return {
		focus: () => view.focus(),
		destroy: () => {
			ended = true;
			view.destroy();
			provider.destroy();
			doc.destroy();
		}
	};
}
