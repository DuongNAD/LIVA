-- Verify a user's subscription and Clerk mirror state
-- Sourced from clerk-sync-tracer skill scripts

-- 1. Check basic profile and Clerk webhook sync flag
SELECT 
    "id", 
    "clerkId", 
    "email", 
    "subscriptionPlanId", 
    "subscriptionStatus", 
    "user_in_clerk", 
    "createdAt"
FROM "Users" 
WHERE "clerkId" = :clerkId OR "email" = :email;

-- 2. Verify all organization memberships and plan mappings for a user
-- (Remember: Org members get "business" plan regardless of personal subscription)
SELECT 
    m."id" AS "membershipId",
    m."userId",
    m."organizationId",
    m."role",
    o."name" AS "organizationName",
    o."org_active",
    u."subscriptionPlanId" AS "userPlanId"
FROM "OrganizationMemberships" m
JOIN "Organizations" o ON m."organizationId" = o."id"
JOIN "Users" u ON m."userId" = u."id"
WHERE u."clerkId" = :clerkId;

-- 3. Check for orphaned active subscriptions
SELECT 
    "id", 
    "clerkId", 
    "email", 
    "subscriptionPlanId", 
    "subscriptionStatus"
FROM "Users"
WHERE "subscriptionStatus" = 'active' 
  AND ("user_in_clerk" IS FALSE OR "user_in_clerk" IS NULL);
