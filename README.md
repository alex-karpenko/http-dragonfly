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

Anyway, to run `http-dragonfly` we need a [configuration file](#concents-and-configuration) with listeners, targets, transformations and response strategy configured.

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

The only mandatory parameter is a path to configuration file. Detailed explanation of all possible configuration options is in the dedicated [Concents and Configuration](#concents-and-configuration) section. Just for test purpose, there is an [example minimal config](config.yaml) file to listen on default 8080 port and forward requests to <http://www.google.com/>. To use it:

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

## Concents and Configuration

Configuration is a `yaml` file with a list of `listeners` as a root element.

### What is a Listener

Each listener has a handler which listens on specific IP and port and do the following:

- accept incoming TCP connections
- get incoming request
- transform it if needed
- verify conditions
- send new request(s) to all configured and allowed targets
- wait for response(s) from all targets
- select obtained or create new response according to configured strategy
- transform it if needed
- and finally send it back to the requester

### Contexts

TODO

### Listener configuration

Each listener has the following configuration parameters:

- `id`: unique name of the listener.
- `listen_on`: IP address and port to listen on.
- `timeout`: time to wait for request/headers/body.
- `methods`: list of allowed HTTP methods to pass through this listener.
- `strategy`: response strategy to select which target(s) to use and which response to send back.
- `headers`: list of transformations to apply to request headers before pass it to targets.
- `targets`: list of targets to query for responses.
- `response`: specification of response transformations.

#### Listener: `id`

Format: string.

Default: `LISTENER-<IP>:<PORT>`

Each listener has to have its own unique name (ID) to distinguish listeners at least in logs. This is an optional parameter with default to `LISTENER-<IP>:<PORT>` where IP and PORT are values from listener's `listen_on` parameter.

So if you omit this parameter it'll be defaulted to unique value but if you prefer to see some reasonable value in logs and config you should set this parameter.

#### Listener: `listen_on`

Format: `<IP>:<PORT>`, IP - any valid IP v4, or `0.0.0.0` or `*` for all host's IP addresses; port is an integer in the range 1..65535.

Default: `0.0.0.0:8080`

It's obvious that each listener accepts connections on its own IP and port. If you have more than one listener in the config you have to specify this parameter at least for all non-default listeners.

#### Listener: `timeout`

Format: human readable time interval, like `5s`, `1m30s`, etc.

Default: `10s`

This time is an interval between accepting incoming connection and getting request's data like headers and/or body. If remote side hasn't sent any data during this interval connection will be dropped without response.

#### Listener: `methods`

Format: list, allowed values are `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, `HEAD`

Default: all method are allowed, though there is no option like `ANY` or `ALL`

This is first filter for incoming requests: all request with method not in the list will be rejected with `405` status (method not allowed). If you want to accept all methods on the listener just don't specify this parameter, that means `everything is allowed`.

Example:

```yaml
methods:
  - GET
  - POST
  - OPTIONS
```

#### Listener: `strategy`

Format: see below list of allowed values.

Default: `failed_then_override`

Strategy is about how to decide which target(s) to query and which response to send back. This is one of the crucial listener's config parameter. Generally all strategies can be divided into four groups by prefixes:

- `always` - regardless of any obtained responses from the targets we should **always** send back something else (override) or unconditional (e.g. response form some specified target)
- `ok` - we respond with **any successful** response if we got at least one successful status from any target, but if **all targets are failed** (regardless of kind of failure) we should return something else
- `failed` - like previous but vise versa: we respond with **any failed** response if we got at least one failure status from any target, but if **all targets are ok** we should return something else
- `conditional_routing` - we query **single target** only which satisfies some condition (see below) and return its response.

| Strategy name         | How it works                                                               |
|-----------------------|----------------------------------------------------------------------------|
| always_override       | Query all allowed targets (see explanation of allowed targets below the table) but return response defined in `response.override` section (see below)|
| always_target_id      | Query all allowed targets but return response from one specific target regardless of it's status|
| ok_then_failed        | Query all allowed targets, if at least on response is successful - return any successful one, if all responses are failed - return any failed response|
| ok_then_target_id     | Query all allowed targets, if at least on response is successful - return any successful one, if all responses are failed - return response from one specific target regardless of it's status|
| ok_then_override      | Query all allowed targets, if at least on response is successful - return any successful one, if all responses are failed - return response defined in `response.override` section (see below)|
| failed_then_ok        | Query all allowed targets, if at least one query is failed - return any failed response, if all responses are successful - return any successful one|
| failed_then_target_id | Query all allowed targets, if at least one query is failed - return any failed response, if all responses are successful - return response from one specific target regardless of it's status|
| failed_then_override  | Query all allowed targets, if at least one query is failed - return any failed response, if all responses are successful - return response defined in `response.override` section (see below). This is default behavior: query everything, return fail if failed or return some predefined OK response if everything is good.|
| conditional_routing   | Select single target to query based on conditions (see targets config below), query it and return it's response.|

Any target may have `condition` parameter which restricts allowance of the target to query it. This condition is predicate based on request's headers or body content. If target's condition is false that target will be excluded from allowed list and won't be queried. Exception is `conditional_routing` strategy where all targets must have conditions but after evaluation of all conditions only single one can be true and only this one target will be queried for response. MOre about conditions configuration is in the targets config section.

#### Listener: `headers`

Format: list of objects.

Default: empty.

This parameter is a list of transformations which should be applied to the original request's headers before proceeding to querying and evaluating targets. Using this config we can add, change or drop some headers from request. Each element of list is a single transformation action, actions will be applied in the list order.

Possible transformations:

- `add` - creates new header with specified name and value, if header already exists - transformation will be ignored.
- `update` - change value of existing header with specified name, if header doesn't exist - transformation will be ignored.
- `drop` - drops header with specified name if it exists. Special case is `*` name which drops all headers.

Examples:

```yaml
headers:
  - add: X-Some-Fun
    value: "123"
  - drop: Content-Type
  - add: Accept
    value: "*"
```

**Important**: if you strictly need to set header to some specific value regardless of header's presence you should add two consecutive actions - `add` and `update` with the same `value`, order doesn't matter: if you try to update non-existent header - new one won't appear, if you try to add existing header - value won't be changed.

If you need to guarantee some stable set of headers instead of requested, just drop all headers (`drop: "*"`) as first action and add all needed ones as following actions.

#### Listener: `targets`

#### Listener: `response`


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
