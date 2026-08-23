import { writable } from 'svelte/store';
import type { InviteData } from '$lib/invite';

/**
 * An invite the user opened but has not accepted yet.
 *
 * The `/invite` route parses the link and hands it over here rather than
 * acting on it: the details live in the URL fragment, which the redirect to
 * the login or server screen drops. Deliberately in-memory only — an invite
 * is consumed within the same page load, and a server password has no reason
 * to sit in storage before the user has agreed to add the server at all.
 */
export const pendingInvite = writable<InviteData | null>(null);
