# Wani

## A familiar WaniKani experience -- in the terminal

- Do your WaniKani lessons and reviews from the command line.
- Work offline, syncing reviews with the WaniKani servers later.
- Familiar UI and keybindings, but even more keyboard-centric.
- Minimizes network requests for a more lag-free experience.

## DISCLAIMER
This is an unofficial WaniKani client. Use at your own risk.

## INSTALL

### From Source
With [rust/cargo](https://www.rust-lang.org/tools/install) installed:
```
cargo install --git https://github.com/wily-james/wani.git
```

### Binaries

TODO

## USE

Get your wanikani summary:
```
wani
```

Check the help:
```
wani -h
```

First-time setup and download subjects:
```
wani sync
```

Do your reviews
```
wani r
```

Do your lessons
```
wani l
```

## CONFIGURATION

### FILE PATH

Wani looks for a config file by default at "~/.config/wani/.wani.conf"  
The containing directory for the ".wani.conf" file can be overridden by specifying the directory path as a command line argument:
```
wani -c /some/path
```

Or by adding the desired path to the WANI_CONFIG_PATH environment variable
```
export WANI_CONFIG_PATH=/some/path
```

### CONFIG OPTIONS

Sample .wani.conf file:

```
auth: your_auth_token
colorblind: true
datapath: /some/path
```

#### Options (all are optional):
Specify your WaniKani personal API token. See https://www.wanikani.com/settings/personal_access_tokens
```
auth: your_auth_token
```
  

Enable some minimal accessibility features for red-green colorblindness.
```
colorblind: true
``` 
  
Specify an alternate location for your local cache of WaniKani subject data.
```
datapath: /some/path
``` 
