import requests
import json
import sys

# We'll use the registration logic from our test suite but in Python to be quick
URL = "http://localhost:8080/api/accounts/register"
EMAIL = "test@example.com"
PASSWORD = "testpassword123"

# Mock values that are base64 encoded
# masterPasswordHash: base64(pbkdf2(password, email, 600000)) -> roughly
# For registration, we can actually send anything, VW will store it.
# BUT to login later with corbw, the hash must match what corbw calculates.

payload = {
    "email": EMAIL,
    "masterPasswordHash": "YVpndHozNmlyREt6TTVXQ3pLclozSXBOWEY4NllIUU56QzhUVE1YQ0lXWT0=", # mock
    "masterPasswordHint": "",
    "key": "YVpndHozNmlyREt6TTVXQ3pLclozSXBOWEY4NllIUU56QzhUVE1YQ0lXWT0=", # mock
    "name": "Test User"
}

res = requests.post(URL, json=payload)
print(f"Status: {res.status_code}")
print(f"Response: {res.text}")
