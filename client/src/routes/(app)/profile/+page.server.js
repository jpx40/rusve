import { getFormValue } from "$lib/utils";
import { safe } from "$lib/safe";
import { grpcSafe } from "$lib/safe";
import { upsendApi } from "$lib/server/api";
import { usersService } from "$lib/server/grpc";
import { logger, perf } from "$lib/server/logger";
import { createMetadata } from "$lib/server/metadata";
import { fail } from "@sveltejs/kit";

/** @type {import('./$types').PageServerLoad} */
export async function load({ parent }) {
    const end = perf("Load profile");
    const profile = await parent();

    /**
     * We return the profile data immediately, and then fetch the resume and stream it to the client as it loads.
     */
    /** @type {Promise<import("$lib/safe").Safe<import("$lib/types").UpsendFile | undefined>>} */
    let resumePromise = Promise.resolve({ data: undefined, error: false });
    if (profile.profile.resumeId) {
        /** @type {Promise<import('$lib/safe').Safe<import('$lib/types').UpsendFile>>} */
        resumePromise = upsendApi({
            url: `/files/${profile.profile.resumeId}`,
            method: "GET",
        });
    }

    end();
    return {
        stream: { resume: resumePromise },
    };
}

/** @type {import('./$types').Actions} */
export const actions = {
    createProfile: async ({ locals, request }) => {
        const end = perf("Create profile");
        const form = await request.formData();

        let resumeId = getFormValue(form, "resumeId");
        const resume = form.get("resume");
        if (!(resume instanceof File)) {
            return fail(400, { error: "Resume must be a PDF" });
        }
        if (resume.size > 0) {
            if (resume.size > 5 * 1024 * 1024) {
                return fail(400, { error: "Resume must be less than 5MB" });
            }
            if (!resume.name.endsWith(".pdf")) {
                return fail(400, { error: "Resume must be a PDF" });
            }

            /**
             * Delete old resume
             */
            if (resumeId) {
                const resDel = await upsendApi({
                    url: `/files/${resumeId}`,
                    method: "DELETE",
                });
                if (resDel.error) {
                    return fail(400, { error: resDel.msg });
                }
            }

            /**
             * Upload new resume
             * @type {import("$lib/safe").Safe<import("$lib/types").UpsendFile>}
             */
            const file = await upsendApi({
                url: "/files",
                method: "POST",
                file: resume,
            });
            if (file.error) {
                return fail(400, { error: file.msg });
            }

            resumeId = file.data.id;
        }

        let coverId = getFormValue(form, "coverId");
        let coverUrl = getFormValue(form, "coverUrl");
        const cover = form.get("cover");
        if (!(cover instanceof File)) {
            return fail(400, { error: "Cover must be an image" });
        }
        if (cover.size > 0) {
            if (cover.size > 5 * 1024 * 1024) {
                return fail(400, { error: "Cover must be less than 5MB" });
            }
            const extensions = [".png", ".jpg", ".jpeg", ".gif", ".svg"];
            if (!extensions.some((ext) => cover.name.endsWith(ext))) {
                return fail(400, { error: "Cover must be an image" });
            }

            /**
             * Delete old cover
             */
            if (coverId) {
                const resDel = await upsendApi({
                    url: `/images/${coverId}`,
                    method: "DELETE",
                });
                if (resDel.error) {
                    return fail(400, { error: resDel.msg });
                }
            }

            /**
             * Upload new cover
             * @type {import("$lib/safe").Safe<import("$lib/types").UpsendImage>}
             */
            const file = await upsendApi({
                url: "/images",
                method: "POST",
                file: cover,
            });
            if (file.error) {
                return fail(400, { error: file.msg });
            }

            coverId = file.data.id;
            coverUrl = file.data.url;
        }

        /** @type {import('$lib/proto/proto/Profile').Profile} */
        const data = {
            id: getFormValue(form, "id"),
            name: getFormValue(form, "name"),
            about: getFormValue(form, "about"),
            resumeId: resumeId,
            coverId: coverId,
            coverUrl: coverUrl,
        };

        /** @type {import("$lib/safe").Safe<import("$lib/proto/proto/Profile").Profile__Output>} */
        const res = await new Promise((r) => {
            usersService.CreateProfile(
                data,
                createMetadata("", locals.user.id),
                grpcSafe(r),
            );
        });

        if (res.error) {
            if (res.fields) {
                return fail(400, { fields: res.fields });
            }
            return fail(400, { error: res.msg });
        }

        /**
         * Send email with the data to the user
         * We don't check for errors cos the upsend API is not critical
         */
        safe(
            upsendApi({
                url: "/emails",
                method: "POST",
                email: {
                    email_to: locals.user.email,
                    email_name: res.data.name,
                    email_subject: "You've updated your profile",
                    email_html: `
                <p>Hi ${res.data.name},</p>
                <p>You've updated your profile. You can view it <a href="https://rusve.bearbyte.org/profile">here</a>.</p>
                <p>Thanks!</p>
                `,
                },
            }),
        ).catch(() =>
            logger.error("Failed to send email to user", {
                email: locals.user.email,
            }),
        );

        end();
        return {
            profile: res.data,
        };
    },
};