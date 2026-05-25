import sqlite3
import glob

db_paths = glob.glob('e:/Project/LIVA/liva-gateway/data/**/*.sqlite', recursive=True)

for db_path in db_paths:
    print(f"\nChecking database: {db_path}")
    try:
        conn = sqlite3.connect(db_path)
        cursor = conn.cursor()
        
        # Check
        cursor.execute("SELECT key FROM facts WHERE key LIKE 'stress_fact_%'")
        rows = cursor.fetchall()
        print(f"Found {len(rows)} stress facts to delete.")
        
        if len(rows) > 0:
            # Delete
            cursor.execute("DELETE FROM facts WHERE key LIKE 'stress_fact_%'")
            conn.commit()
            print("Successfully deleted stress facts from the database!")
            
            # Verify
            cursor.execute("SELECT key FROM facts WHERE key LIKE 'stress_fact_%'")
            rows_after = cursor.fetchall()
            print(f"Verification: Found {len(rows_after)} stress facts remaining.")
            
        conn.close()
    except Exception as e:
        print(f"Error: {e}")
