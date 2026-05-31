import { QueryInterface, DataTypes } from 'sequelize';

/**
 * Migration NNN: [Component / Feature Name] — [Short Description]
 *
 * Adds/Modifies:
 * - [Details of schema adjustments]
 *
 * Purpose:
 * - [Reason for this migration, context alignment]
 */

export async function up(queryInterface: QueryInterface): Promise<void> {
    console.log('[NNN_migration_name] Applying database schema updates...');

    // Example 1: Creating a compound index
    // await queryInterface.addIndex('TableNames', ['columnOne', 'columnTwo'], {
    //     name: 'idx_tablenames_colone_coltwo',
    //     unique: false,
    // });

    // Example 2: Adding a camelCase column
    // await queryInterface.addColumn('TableNames', 'newColumnName', {
    //     type: DataTypes.STRING,
    //     allowNull: true,
    // });

    console.log('[NNN_migration_name] ✅ Migration applied successfully.');
}

export async function down(queryInterface: QueryInterface): Promise<void> {
    console.log('[NNN_migration_name] Reverting database schema updates...');

    // Reversal of example 2
    // await queryInterface.removeColumn('TableNames', 'newColumnName');

    // Reversal of example 1
    // await queryInterface.removeIndex('TableNames', 'idx_tablenames_colone_coltwo');

    console.log('[NNN_migration_name] ✅ Rollback completed.');
}
