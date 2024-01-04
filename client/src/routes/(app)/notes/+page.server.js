import { getFormValue } from "$lib/utils";
import { grpcSafe, safe } from "$lib/safe";
import { notesService } from "$lib/server/grpc";
import { createMetadata } from "$lib/server/metadata";
import { fail } from "@sveltejs/kit";

/** @type {import('./$types').PageServerLoad} */
export async function load({ locals, url }) {
    const metadata = createMetadata(locals.user.id);

    // Count notes
    /** @type {Promise<import("$lib/safe").Safe<import("$lib/proto/proto/Count").Count__Output>>} */
    const s1 = new Promise((r) => {
        notesService.CountNotesByUserId({}, metadata, grpcSafe(r));
    });

    // Get notes
    const offset = (Number(url.searchParams.get("p")) || 1) - 1;
    const limit = 1;
    const notesStream = notesService.GetNotesByUserId(
        {
            offset: offset * limit,
            limit,
        },
        metadata,
    );
    /** @type {Promise<import("$lib/proto/proto/Note").Note__Output[]>} */
    const p2 = new Promise((res, rej) => {
        /** @type {import("$lib/proto/proto/Note").Note__Output[]} */
        const notes = [];
        notesStream.on("data", (data) => notes.push(data));
        notesStream.on("error", (err) => rej(err));
        notesStream.on("end", () => res(notes));
    });
    const s2 = safe(p2);

    // Wait for both
    const [d1, d2] = await Promise.all([s1, s2]);

    if (d1.error) {
        return {
            error: d1.msg,
            notes: [],
            total: 0,
            pageSize: limit,
        };
    }
    if (d2.error) {
        return {
            error: d2.msg,
            notes: [],
            total: 0,
            pageSize: limit,
        };
    }

    return {
        notes: d2.data.sort(
            (a, b) =>
                new Date(b.created).getTime() - new Date(a.created).getTime(),
        ),
        total: Number(d1.data.count),
        pageSize: limit,
    };
}

/** @type {import('./$types').Actions} */
export const actions = {
    insert: async ({ locals, request }) => {
        const form = await request.formData();

        /** @type {import("$lib/proto/proto/Note").Note} */
        const data = {
            title: getFormValue(form, "title"),
            content: getFormValue(form, "content"),
        };
        const metadata = createMetadata(locals.user.id);
        /** @type {import("$lib/safe").Safe<import("$lib/proto/proto/Note").Note__Output>} */
        const req = await new Promise((r) => {
            notesService.CreateNote(data, metadata, grpcSafe(r));
        });

        if (req.error) {
            if (req.fields) {
                return fail(400, { fields: req.fields });
            }
            return fail(500, { error: req.msg });
        }

        return { note: req.data };
    },
};
