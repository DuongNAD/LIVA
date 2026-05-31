import { OAuth2Client } from 'google-auth-library';
import { clerkClient } from '@clerk/express';

/**
 * Example demonstrating how native Android Google OAuth tokens are verified 
 * and exchanged for a short-lived Clerk Sign-In Ticket on the backend.
 * 
 * Target Route: POST /api/auth/native-login
 * Mounted before auth middleware in server/index.ts.
 */
export async function handleNativeGoogleAuthExchange(idToken: string) {
    const webClientId = process.env.GOOGLE_WEB_CLIENT_ID;
    if (!webClientId) {
        throw new Error('GOOGLE_WEB_CLIENT_ID is not configured in server environment.');
    }

    // 1. Verify Google Identity Token
    const oauthClient = new OAuth2Client(webClientId);
    const ticket = await oauthClient.verifyIdToken({
        idToken,
        audience: webClientId,
    });

    const payload = ticket.getPayload();
    if (!payload || !payload.email) {
        throw new Error('Invalid Google payload: missing email address.');
    }

    const { email, given_name: firstName, family_name: lastName } = payload;

    // 2. Fetch or Create the corresponding User inside Clerk
    let clerkUser;
    const existingUsers = await clerkClient.users.getUserList({ emailAddress: [email] });

    if (existingUsers.data.length > 0) {
        clerkUser = existingUsers.data[0];
    } else {
        // Create new password-less user mirror in Clerk
        clerkUser = await clerkClient.users.createUser({
            emailAddress: [email],
            firstName: firstName ?? undefined,
            lastName: lastName ?? undefined,
            skipPasswordRequirement: true,
        });
    }

    // 3. Generate a short-lived Clerk Sign-In Ticket (expiring in 5 mins)
    const signInToken = await clerkClient.signInTokens.createSignInToken({
        userId: clerkUser.id,
        expiresInSeconds: 300,
    });

    // 4. Return the ticket to the native client
    // Native React client will then complete sign-in using:
    // signIn.create({ strategy: 'ticket', ticket: data.ticket })
    return { ticket: signInToken.token };
}
