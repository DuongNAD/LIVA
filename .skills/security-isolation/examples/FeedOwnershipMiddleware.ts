import { Request, Response, NextFunction } from 'express';
import { Feed } from '../../../server/models/index.js'; // Correct relative path from .skills/
import { WhereOptions } from 'sequelize';

interface ExtendedRequest extends Request {
    user?: { id: string; role: string };
    feed?: InstanceType<typeof Feed>;
    log?: { error: (obj: object, msg: string) => void };
}

/**
 * Example showcasing how the standard Feed Ownership middleware
 * blocks Insecure Direct Object Reference (IDOR) attacks.
 * 
 * Sourced from: server/middleware/feedOwnership.ts
 */
export const withFeedOwnershipExample = async (
    req: ExtendedRequest,
    res: Response,
    next: NextFunction
): Promise<void> => {
    try {
        const feedId = req.params.id;

        // 1. Establish the base query parameter
        const whereClause: WhereOptions = { id: feedId };

        // 2. Tenant Isolation Guard:
        // Scopes the query by the user ID from the session unless they hold 'Admin' role.
        if (req.user!.role !== 'Admin') {
            whereClause.userId = req.user!.id;
        }

        // 3. Query PostgreSQL
        const feed = await Feed.findOne({
            where: whereClause
        });

        // 4. Return 404 on absence or ownership mismatch
        // Prevents users from verifying the existence of other users' feeds.
        if (!feed) {
            res.status(404).json({ error: 'Feed not found' });
            return;
        }

        // 5. Attach resolved record to the request context for subsequent route handlers
        req.feed = feed;
        next();

    } catch (error) {
        req.log?.error({ err: error }, 'Failed to check feed ownership');
        res.status(500).json({ error: 'Server authorization check failed' });
    }
};
