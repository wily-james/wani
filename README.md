# Wani

## A familiar WaniKani experience. . . in the terminal

- Do your WaniKani reviews from the command line.
- Work offline, syncing reviews with the WaniKani servers later.
- Familiar UI and keybindings, but even more keyboard-centric.
- Minimizes network requests for a more lag-free experience.

## INSTALL

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

Option explanations:
```
auth: your_auth_token
```
Specify your WaniKani personal API token. See https://www.wanikani.com/settings/personal_access_tokens
  

```
colorblind: true
``` 
Enable some minimal accessibility features for red-green colorblindness.
  

```
datapath: /some/path
``` 
Specify an alternate location for your local cache of WaniKani subject data.
