# THIS IS DRAFT ONLY! DON'T USE IT!

# http-dragonfly

This is a tiny service app to redirect, split or relay HTTP calls to one or more destinations based on flexible configuration.

## Features

- Listen on one or more IP/ports pairs to serve calls. Each listener has its own configuration. Number of listeners is unrestricted.
- Relay HTTP requests to one or more targets based on reach and flexible configuration set.
- Filter/restrict requests by methods, headers, body content and route it to all or conditionally selected targets.
- Transform request's headers, path and body in a flexible configurable way.
- Decide which response to send back based on the configured response strategy.
- Transform response headers and body.
- Propagate target's response status/headers/body or overwrite it.

## Usage

The easiest way to run exporter is to use [Docker image](#docker-image). If you use Kubernetes to run workload you can use [Helm chart](#helm-chart) to configure and deploy exporter. The third way to run exporter is to [build native Rust binary](#build-your-own-binary) using Cargo utility and run it.

Anyway, to run `http-dragonfly` we need a [configuration file](#configuration) with listeners, targets, transformations and response strategy configured.

### Docker image

Just use the following command to get usage help, the same as running it with `--help` command line option:

```bash
docker run --rm alexkarpenko/http-dragonfly:latest
```

Typical output is:

```console
HTTP requests splitter/router/relay

Usage: http-dragonfly [OPTIONS] --config <CONFIG>

Options:
  -d, --debug
          Enable extreme logging (debug)
  -v, --verbose
          Enable additional logging (info)
  -j, --json-log
          Write logs in JSON format
  -c, --config <CONFIG>
          Path to config file
  -e, --env-mask <ENV_MASK>
          Allowed environment variables mask (regex) [default: ^HTTP_ENV_[a-zA-Z0-9_]+$]
      --health-check-port <HEALTH_CHECK_PORT>
          Enable health check responder on the specified port
  -h, --help
          Print help
  -V, --version
          Print version
```

The only mandatory parameter is a path to configuration file. Detailed explanation of all possible configuration options is in the dedicated [Configuration](#configuration) section. Just for test purpose, there is an [example minimal config](config.yaml) file to listen on default 8080 port and forward requests to <http://www.google.com/>. To use it:

```bash
docker run --rm --name http-dragonfly -v $PWD/config.yaml:/config.yaml alex-karpenko/http-dragonfly:latest --config /config.yaml -v
```

### Helm chart

To add Helm repository:

```bash
helm repo add alex-karpenko https://alex-karpenko.github.io/helm-charts
helm repo update
```

To deploy release create your own values file with overrides of the default values and your own config section and deploy Helm release to your K8s cluster:

```bash
helm install http-dragonfly alex-karpenko/http-dragonfly -f my-values.yaml
```

For example your values can be like below.

```yaml
# info or debug, anything else - warning
logLevel: info

service:
  type: ClusterIP
  healthCheck:
    port: 3000 # actually this port is for health checks only
    expose: false
  listeners: # these ports are for listeners
  - port: 8000
    name: test-listener-1 # name is optional

config:
  listeners:
    - id: test-google
      listen_on: "*:8000"
      timeout: 5s
      methods:
      - GET
      strategy: failed_then_override
      targets:
        - url: https://www.google.com/
          id: google
      response:
        override:
          headers:
          - add: x-reposonse-target-id
            value: ${CTX_TARGET_ID}
```

### Build your own binary

Since exported is written in Rust, you can use standard Rust tools to build binary for any platform you need. Of course, you have to have [Rust](https://rust-lang.org) tool-chain installed.

```bash
cargo build --release
```

And run it:

```bash
target/release/http-dragonfly --config ./config.yaml -v
```

## Configuration

Configuration file is `yaml` file with list on `listeners` at root.

### THIS IS DRAFT EXAMPLE FILE! DON'T USE IT! THIS SECTION WILL BE WRITTEN LATER!

#### Detailed configuration with explanation

Below is a detailed explanation of all possible configuration parameters. Default values for optional parameters are specified.


```yaml
listeners:
  - id: Listener-8080 # default is LISTENER-<on value>
    listen_on: "*:8080" # or ip:port like 1.2.3.4:1234, or just port number
    timeout: 10s
    methods: # default - empty list that means "any method"
      - GET
      - POST
    # always_override
    # always_target_id
    # ok_then_failed
    # ok_then_target_id
    # ok_then_override
    # failed_then_ok
    # failed_then_target_id
    # failed_then_override - default
    # conditional_routing
    strategy: failed_then_override # default
    #strategy: always_override

    # Add: if exists - preserve value, else - add it
    # Update: if exists - update value, else - preserve it
    # Drop: if exists - drop it
    headers:
      - drop: "*" # special case, default is preserve everything except Host
      - add: X-Added-Header
        value: something
      - update: Authorization
        value: ${SOME_AUTH_TOKEN}
      - drop: X-Forwarded-For
      - update: User-Agent
        value: "ha-ha-ha-${CTX_REQUEST_HEADERS_USER_AGENT}"

    targets:
      - id: Target-0 # default is TARGET-<url value>
        url: https://qqq.www.com/
        timeout: 60s
        body: '{"method": "${CTX_REQUEST_METHOD}"}'
        on_error: propagate # default, possible values are `propagate|status|drop`
        #error_status: 500 Internal error
      - id: Target-1
        condition: .body.target == "1"
        url: https://qqq.www.com/${CTX_REQUEST_PATH}
      - id: Target-2
        url: https://qqq.www.com/${CTX_REQUEST_PATH}?${CTX_REQUEST_QUERY}
        headers:
          - update: Authorization
            value: ${TARGET_2_AUTH_TOKEN}
          - drop: X-Added-Header

    response:
      target_selector: Target-0 # target.id for *_targer_id strategies
      failed_status_regex: "4\\d{2}|5\\d{2}"
      no_targets_status: 500

      override:
        # "200 OK" is default for *_override
        # for other strategies - no default, so status will be preserved from response or overridden if value is defined
        status: 200
        # empty is default for *_override
        # ${CTX_RESPONSE_BODY} is for other strategies
        body: |
          {"status": "ok"}
        headers: # default is to preserve original headers or empty if strategy is one of *_override
          - update: content-type
            value: application/json
          - add: X-Http-Splitter-Version
            value: ${CTX_APP_VERSION}
          - add: X-Http-Splitter-Response-Source
            value: ${CTX_TARGET_ID}

  - id: Condition-plus-target_id-8081
    listen_on: "*:8081"
    timeout: 30s
    strategy: always_target_id
    methods:
      - PUT
      - POST
    headers:
      - add: X-Added-Header
        value: router-${CTX_TARGET_ID}
    targets:
      - id: Target-0
        condition: .body.target == "0"
        url: https://qqq.www.com/
        timeout: 60s
      - id: Target-1
        condition: .headers["Qqq"] == "WWW"
        url: https://qqq.www.com/${CTX_REQUEST_PATH}
      - id: Target-2
        condition: default
        url: https://qqq.www.com/${CTX_REQUEST_PATH}?${CTX_REQUEST_QUERY}
        headers:
          - add: X-Http-Gorgona
            value: default target
    response:
      target_selector: Target-0

      override:
        headers:
          - add: X-Http-Conditional-Response-Source
            value: ${CTX_TARGET_ID}

  - id: Router-8082
    listen_on: "*:8082"
    timeout: 30s
    strategy: conditional_routing
    methods:
      - PUT
      - POST
    headers:
      - add: X-Added-Header
        value: router-${CTX_TARGET_ID}
    targets:
      - id: Target-0
        condition: .body.target == "0"
        url: https://qqq.www.com/
        timeout: 60s
      - id: Target-1
        condition: .headers["Qqq"] == "WWW"
        url: https://qqq.www.com/${CTX_REQUEST_PATH}
      - id: Target-2
        condition: default
        url: https://qqq.www.com/${CTX_REQUEST_PATH}?${CTX_REQUEST_QUERY}
        headers:
          - add: X-Http-Gorgona
            value: default target
    response:
      no_targets_status: 555

      override:
        headers:
          - add: X-Http-Conditional-Response-Source
            value: ${CTX_TARGET_ID}
```
