import sys
import zipfile
import os

def verify_apk(apk_path):
    print(f"Verifying APK at: {apk_path}")
    if not os.path.exists(apk_path):
        print(f"Error: File does not exist: {apk_path}")
        return False
        
    if not zipfile.is_zipfile(apk_path):
        print(f"Error: File is not a valid zip archive: {apk_path}")
        return False
        
    required_entries = ["AndroidManifest.xml", "classes.dex", "resources.arsc"]
    missing_entries = []
    
    with zipfile.ZipFile(apk_path, 'r') as jar:
        file_list = jar.namelist()
        print("Found files in zip archive:")
        for name in file_list[:20]:  # print first 20 entries
            print(f" - {name}")
        if len(file_list) > 20:
            print(f" ... and {len(file_list) - 20} more entries")
            
        for entry in required_entries:
            if entry not in file_list:
                missing_entries.append(entry)
                
        # Check for signature files in META-INF/
        signature_files = [
            name for name in file_list
            if name.startswith("META-INF/") and (
                name.endswith(".RSA") or name.endswith(".DSA") or name.endswith(".EC") or name.endswith(".SF")
            )
        ]
                
    if missing_entries:
        print(f"Error: Missing standard Android app entries: {missing_entries}")
        return False
        
    if not signature_files:
        print("Error: APK is unsigned (no signature files found in META-INF/)")
        return False
    else:
        print(f"Found signature files: {signature_files}")
        
    print("Success: APK is a valid zip archive, contains all standard Android app entries, and is signed.")
    return True

if __name__ == "__main__":
    apk_path = r"E:\Project\LIVA\release\liva-mobile.apk"
    if len(sys.argv) > 1:
        apk_path = sys.argv[1]
    
    success = verify_apk(apk_path)
    if not success:
        sys.exit(1)
    sys.exit(0)
