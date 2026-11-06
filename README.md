<p align="center">
    <img alt="the while86 logo" width="300" src="logo.svg">
</p>

A no-nonsense [BitTorrent Tracker](https://en.wikipedia.org/wiki/BitTorrent_tracker), with dead simple whitelisting to stop your tracker from being used for unauthorised purposes. Magnet Box also features a Web Interface for browsing, uploading and downloading tracked torrents.

# Quickstart

Recommended installation using [Docker Compose](https://docs.docker.com/compose/). You can find the latest Magnet Box image at `ghcr.io/brooke-ec/magnet-box:latest`. An example configuration file can be seen below:

```yml
services:
    magnet-box:
        build: .
        ports:
            - 8080:8080
        restart: unless-stopped
        volumes:
            - ./data:/data
        environment:
            ROCKET_SECRET_KEY: "this_will_error_see_comment" # generate with `openssl rand -base64 32`
            ROCKET_URL: "https://example.com" # replace with your domain
```

The application logs will display a link to update the admin account's password. After doing so, you can upload your first torrent!
