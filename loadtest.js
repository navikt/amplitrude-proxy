import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
    vus: 100, // Virtual users, Carl
  duration: '30s',
};

export default function () {
    // This is against mcchouten/go-http-bin , not socat (It was what i had running when i wrote this)
  const url = 'http://localhost:6191/dump/request';
  const payload = JSON.stringify({
    "api_key": "7883e1aasdfa3w13417",
    "events": [
      {
        "device_id": "PROXYPROXYPROXY",
        "session_id": 1727432370360,
        "time": 1727432412393,
        "platform": "CARL",
        "os_name": "CARL",
        "os_version": "CARL",
        "device_model": "CARL",
        "language": "nb-NO",
        "ip": "$remote",
        "insert_id": "98c6079b-1868-4d5d-8f23-725a7f5a4bf8",
        "event_type": "[Amplitude] PROXYPROXYPROXY",
        "event_properties": {
          "utm_campaign": "23031510135",
          "utm_medium": "23031510135",
          "utm_source": "23031510135",
          "referrer": "https://login.microsoftonline.com/",
          "referring_domain": "login.microsoftonline.com",
          "[Amplitude] Page Domain": "lekk.ansatt.nav.no",
          "[Amplitude] Page Location": "https://lekk.ansatt.nav.no/profil/kari-nordmann/23031510135?name=kari-nordmann&fnr=23031510135&utm_source=23031510135&utm_medium=23031510135&utm_campaign=23031510135",
          "[Amplitude] Page Path": "/profil/kari-nordmann/23031510135",
          "[Amplitude] Page Title": "Lekk - Kari Nordmann (fnr: 23031510135)",
          "[Amplitude] Page URL": "https://lekk.ansatt.nav.no/profil/kari-nordmann/23031510135"
        },
        "event_id": 0,
        "library": "amplitude-ts/1.9.1"
      }
    ],
    "options": {}
  });

  const params = {
    headers: {
      'Content-Type': 'application/json',
	'Accept': 'application/json',
	'User-Agent': 'CARL',  // We reject all bot like request, thankfully Carl is not a bot
    },
  };

  const res = http.post(url, payload, params);

  check(res, {
    'is status 200': (r) => r.status === 200,
  });

  sleep(1);
}
