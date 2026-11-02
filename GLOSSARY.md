# lueur

Syncs Philips Hue lights with the video playing on Jellyfin, using color tracks computed ahead of time.

## Hue

**Bridge**:
The Philips Hue hub on the local network that lueur pairs with and streams to.
_Avoid_: Hub, pont (in code/docs)

**Pairing**:
The one-time exchange, triggered by pressing the Bridge's link button, that gives lueur its credentials.
_Avoid_: Linking, registration

**Area**:
A Hue entertainment area: the set of lights lueur drives during playback.
_Avoid_: Room, zone, group, entertainment configuration

**Channel**:
One positioned light slot inside an Area, colored from the matching part of the screen.
_Avoid_: Light, bulb

## Media

**Library**:
A Jellyfin library (movies, shows) that lueur follows, paired with the folder where lueur sees its files.
_Avoid_: Folder, collection, path_map

**Scan**:
A walk through all Libraries that queues an Analysis for every video without an up-to-date Track, and deletes Orphan Tracks.
_Avoid_: Sync, refresh, import

**Auto scan**:
A Scan lueur runs on its own at a regular interval, so new videos get their Track without anyone asking.
_Avoid_: Watcher, sync, cron

**Orphan Track**:
A Track whose video no longer exists (deleted, or replaced by a better version under another name).
_Avoid_: Stale track, dead track

**Analysis**:
The one-time decoding of a video file into its Track.
_Avoid_: Scan, processing, indexing

**Track**:
The precomputed sequence of small color grids for one video file.
_Avoid_: Color file, profile

**Store**:
The folder holding all Tracks.
_Avoid_: Cache, library

**Look**:
The user's color tuning applied at playback (brightness, saturation, smoothing, offset).
_Avoid_: Settings, profile
