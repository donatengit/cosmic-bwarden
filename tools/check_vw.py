import requests
import base64
import sys

# Simplified registration script
URL = "http://localhost:8080/api/accounts/register"
EMAIL = "test@example.com"
# For testing purposes, we can use a pre-calculated hash or just a mock if registration allows it
# Bitwarden registration usually requires: Email, MasterPasswordHash, Key, Name
# Since we just want to test if it's up, let's try a simple post

payload = {
    "email": EMAIL,
    "masterPasswordHash": "some_base64_hash", # This will likely fail validation if VW is strict
    "masterPasswordHint": "",
    "key": "some_base64_key",
    "name": "Test User"
}

try:
    # First, let's check if the server is actually reachable
    requests.get("http://localhost:8080/alive")
    print("Vaultwarden is ALIVE")
except Exception as e:
    print(f"Vaultwarden is NOT reachable: {e}")
    sys.exit(1)
