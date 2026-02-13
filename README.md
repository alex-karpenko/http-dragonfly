# http-dragonfly

<p>
<a href="https://github.com/alex-karpenko/http-dragonfly/actions/workflows/ci.yaml" rel="nofollow"><img src="https://img.shields.io/github/actions/workflow/status/alex-karpenko/http-dragonfly/ci.yaml?label=ci" alt="CI status"></a>
<a href="https://github.com/alex-karpenko/http-dragonfly/actions/workflows/audit.yaml" rel="nofollow"><img src="https://img.shields.io/github/actions/workflow/status/alex-karpenko/http-dragonfly/audit.yaml?label=audit" alt="Audit status"></a>
<a href="https://github.com/alex-karpenko/http-dragonfly/actions/workflows/publish-image.yaml" rel="nofollow"><img src="https://img.shields.io/github/actions/workflow/status/alex-karpenko/http-dragonfly/publish-image.yaml?label=publish" alt="Docker image publishing status"></a>
<a href="https://app.codecov.io/github/alex-karpenko/http-dragonfly" rel="nofollow"><img src="https://img.shields.io/codecov/c/github/alex-karpenko/http-dragonfly" alt="License"></a>
<a href="https://github.com/alex-karpenko/http-dragonfly/blob/HEAD/LICENSE" rel="nofollow"><img src="https://img.shields.io/github/license/alex-karpenko/http-dragonfly" alt="License"></a>
</p>

This is a tiny service app to redirect, split or relay HTTP calls to one or more destinations based on flexible
configuration.

## Features

- Listen to one or more IP/ports pairs to serve calls.
  Each listener has its own configuration.
  The number of listeners is unlimited.
- Relay HTTP requests to one or more targets based on a reach and flexible configuration set.
- Filter/restrict requests by methods, headers, body content and route it to all or conditionally selected targets.
- Transform request's headers, path and body in a flexible configurable way.
- Decide which response to send back based on the configured response strategy.
- Transform response headers and body.
- Propagate target's response status/headers/body or overwrite it.

## Some typical use cases

Actually use cases are restricted by your fantasy only :)

**Debugging**:

- duplicate all queries from production environment to one or more test environments to evaluate new functionality with
  real production requests and workload
- duplicate specific conditionally selected queries to another target to analyze some issues dependent on content
- send web-hooks to several (test) environments simultaneously

**Logging/observing/learning**:

- duplicate queries to different log storages without affecting of normal request flow: i.e., log body to ElasticSearch,
  source IPs and user agent to some simple logger
- duplicate some conditionally selected queries (for example, based on source IP or query string) to special endpoint to
  alert about an abuse or intrusion attempt
- stream all (or selected) requests to machine learning system without affecting normal processing

**Routing**:

- route request to specific target based on query content

## Usage

The easiest way to run `http-dragonfly` is to use [Docker image](#docker-image).
If you use Kubernetes to run workload, you can use [Helm chart](#helm-chart) to configure and deploy `http-dragonfly`.
The third way to run `http-dragonfly` is to [build native Rust binary](#build-your-own-binary) using Cargo utility and
run it.

Anyway, to run `http-dragonfly` we need a [configuration file](#concepts-and-configuration) with listeners, targets,
transformations and response strategy configured.

### Docker image

Use the following command to get usage help, the same as running it with `--help` command line option:

```bash
docker run --rm ghcr.io/alex-karpenko/http-dragonfly:latest
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
  -p, --health-check-port <HEALTH_CHECK_PORT>
          Enable health check responder on the specified port
  -h, --help
          Print help
  -V, --version
          Print version
```

The only mandatory parameter is a path to configuration file.
Detailed explanation of all possible configuration options is in the
dedicated [Concepts and Configuration](#concepts-and-configuration) section.
Just for test purpose, there is
an [example minimal config](config.yaml) file to listen on default 8080 port and forward requests
to <http://www.google.com/>.
To use it:

```bash
docker run --rm --name http-dragonfly -v $PWD/config.yaml:/config.yaml ghcr.io/alex-karpenko/http-dragonfly:latest --config /config.yaml -v
```

### Helm chart

To add Helm repository:

```bash
helm repo add alex-karpenko https://alex-karpenko.github.io/helm-charts
helm repo update
```

To deploy release,
create your own values file with overrides of the default values and your own config section
and deploy Helm release to your K8s cluster:

```bash
helm install http-dragonfly alex-karpenko/http-dragonfly -f my-values.yaml
```

For example, your values can be like below.

```yaml
# info or debug, anything else - warning
logLevel: info

service:
  type: ClusterIP
  healthCheck:
    port: 3000 # actually, this port is for health checks only
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
      strategy: failed_then_ok
      targets:
        - url: https://www.google.com/
          id: google
      response:
        override:
          headers:
            - add: x-response-target-id
              value: ${CTX_TARGET_ID}
```

### Build your own binary

Since `http-dragonfly` is written in Rust, you can use standard Rust tools to build binary for any platform you need.
Of course, you have to have [Rust](https://rust-lang.org) tool-chain installed.

```bash
cargo build --release
```

And run it:

```bash
target/release/http-dragonfly --config ./config.yaml -v
```

## Concepts and Configuration

Configuration is a `yaml` file with a list of `listeners` as a root element.

### Listener

Each listener has a handler which listens to specific IP and port and does the following:

- accept incoming TCP connections
- get incoming request
- transform it if needed
- verify conditions
- send new request(s) to all configured and allowed targets
- wait for response(s) from all targets
- select one of the obtained or create new response, according to configured strategy
- transform it if needed
- and finally send it back to the requester

### Contexts

Context is a set of variables (like Unix environment variables) attached to each request.
Those variables define request's environment
that can be used almost everywhere in a configuration file to substitute parameters and actually to impact configuration
and request's content on the fly.

Just a few examples of how to use contexts:

- insert Authorization header from OS environment variable instead of committing secret value to config.
- change target's URI based on original URI parameters or headers like add path or query string.
- insert some information about query into request or response headers: selected target, source IP, etc.
- create request or response body based on request headers or OS environment

There are four contexts depending on the query stage:

| Context type | Variables                                    | Description                                                                                                                           |
| ------------ | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Application  | CTX_APPLICATION_NAME                         | Name of this app, `http-dragonfly`                                                                                                    |
|              | CTX_APPLICATION_VERSION                      | Version of the app                                                                                                                    |
|              | OS environment variables                     | All OS environment variables which names satisfy restriction mask from the command line (default mask is `^HTTP_ENV_[a-zA-Z0-9_]+$]`) |
| Request      | CTX_LISTENER_NAME                            | ID of the listener which accepted the request                                                                                         |
|              | CTX_REQUEST_SOURCE_IP                        | Client's source IP address                                                                                                            |
|              | CTX_REQUEST_METHOD                           | Request method                                                                                                                        |
|              | CTX_REQUEST_HOST                             | URL host name from the original request                                                                                               |
|              | CTX_REQUEST_PATH                             | URL path from the original request (without leading slashes!)                                                                         |
|              | CTX_REQUEST_QUERY                            | URL query string from the original request                                                                                            |
|              | CTX_REQUEST_HEADERS_<UPPERCASE_HEADER_NAME>  | Each request's header has it's context variable                                                                                       |
| Target       | CTX_TARGET_ID                                | ID of the target which response will be returned back                                                                                 |
|              | CTX_TARGET_HOST                              | Host name of the selected target                                                                                                      |
| Response     | CTX_RESPONSE_HEADERS_<UPPERCASE_HEADER_NAME> | Each response's header has it's context variable                                                                                      |
|              | CTX_RESPONSE_STATUS                          | Status returned by target query                                                                                                       |

To use context variables in the config just specify it similar to the `bash` variables (all shell expressions work).
Few obvious examples, more realistic examples you can see in this file below:

- `${CTX_APPLICATION_NAME}`
- `${CTX_REQUEST_QUERY:-}`
- `${CTX_TARGET_ID:-unknown}`

***Important notes:***

> - Target and responses context's variables are undefined in response context for strategies like `*_override` because
    there is no response target exists.
> - Each context includes context of previous stage: request includes application, target includes request, response
    includes target (except above note).
> - Using of OS environment variables is restricted to specified mask to avoid including of all app environment to the
    context and unintentional exposing of the run-time environment state.
> - If context variable is not defined and there is no default value in the expression, then error won't be raised, and
    variable won't be substituted, and the whole expression will be left as it is.
> - There are some special cases where context variables can't be used and will be ignored (lft as is), see notes in
    particular configuration sections.

Variables substitution stages:

- `application` context applies to a whole config file just after loading before parsing and validating.
- `request` context applies to each request after receiving before headers and body transformations.
- `target` context applies to its target before target's headers, body and URL transformation.
- `response` context applies during a response override process before headers and body transformation.

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

Default: `LISTENER-<IP>:<PORT>` where IP and PORT are values from `listen_on` parameter.

Each listener has to have its own unique name (ID) to distinguish listeners at least in logs.
This is an optional parameter with default
to `LISTENER-<IP>:<PORT>` where IP and PORT are values from listener's `listen_on` parameter.

So if you omit this parameter,
it'll be defaulted to unique value, but if you prefer
to see some reasonable value in logs and config, you should set this parameter.

#### Listener: `listen_on`

Format: `<IP>:<PORT>`, IP — any valid IP v4, or `0.0.0.0` or `*` for all host's IP addresses;
port is an integer in the range 1..65535.

Default: `0.0.0.0:8080`

Each listener accepts connections on its own IP and port.
If you have more than one listener in the config,
you have to specify this parameter at least for all non-default listeners.

#### Listener: `tls`

Format: object with two fields: `verify` and `ca`.

Default:

```yaml
  tls:
    verify: yes
    ca: null
```

This object specifies how to process outgoing TLS connections.
`verify` field sets server certificate verification mode, possible values:
- `yes`: verify server certificate (validity and hostname), default;
- `no`: skip TLS verification, generally this is a dangerous setup.

`ca` field is used to specify a path to the file with custom root CA certificates bundle in PEM format to use instead of
the system one.

So the default TLS verification behavior is:

- skip TLS verification if it's disabled in listener or target config (`tls.verify: no`);
- else, use a custom root CA certificate bundle (file in PEM format) if it's defined in listener or target config
  (`tls.verify: yes` and `tls.ca` has a path to the file);
- else, use OS native certificates bundle if it's present;
- else, use Mozilla root CA bundle.

Example:

```yaml
tls:
  verify: yes
  ca: /custom_ca.pem
```

#### Listener: `timeout`

Format: human readable time interval, like `5s`, `1m30s`, etc.

Default: `10s`

This time is an interval between accepting incoming connection and getting request's data like headers and/or body.
If the remote side hasn't sent any data during this interval connection will be dropped without a response.

#### Listener: `methods`

Format: list, allowed values are `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, `HEAD`

Default: empty — all methods are allowed, although there is no option like `ANY` or `ALL`

This is the first filter for incoming requests:
all requests with method not in the list will be rejected with `405` status (method not allowed).
If you want to accept all methods on the listener just don't specify this parameter, that means `everything is allowed`.

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

Strategy is about how to decide which target(s) to query and which response to send back.
This is one of the crucial listener's config parameters.
Generally, all strategies can be divided into four groups by prefixes:

- `always` - regardless of any obtained responses from the targets we should **always** send back something else (override) or unconditional (e.g., response form some specified target)
- `ok` - we respond with **any successful** response if we got at least one successful status from any target, but if *
  *all targets are failed** (regardless of kind of failure) we should return something else
- `failed` - like previous but vise versa: we respond with **any failed** response if we got at least one failure status
  from any target, but if **all targets are ok** we should return something else
- `conditional_routing` - we query **single target** only which satisfies some condition (see below) and return its
  response.

| Strategy name         | How it works                                                                                                                                                                                                                                                                                                                  |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| always_override       | Query all allowed targets (see explanation of allowed targets below the table) but return response defined in `response.override` section (see below)                                                                                                                                                                         |
| always_target_id      | Query all allowed targets but return response from one specific target regardless of it's status                                                                                                                                                                                                                              |
| ok_then_failed        | Query all allowed targets, if at least on response is successful - return any successful one, if all responses are failed - return any failed response                                                                                                                                                                        |
| ok_then_target_id     | Query all allowed targets, if at least on response is successful - return any successful one, if all responses are failed - return response from one specific target regardless of it's status                                                                                                                                |
| ok_then_override      | Query all allowed targets, if at least on response is successful - return any successful one, if all responses are failed - return response defined in `response.override` section (see below)                                                                                                                                |
| failed_then_ok        | Query all allowed targets, if at least one query is failed - return any failed response, if all responses are successful - return any successful one                                                                                                                                                                          |
| failed_then_target_id | Query all allowed targets, if at least one query is failed - return any failed response, if all responses are successful - return response from one specific target regardless of it's status                                                                                                                                 |
| failed_then_override  | Query all allowed targets, if at least one query is failed - return any failed response, if all responses are successful - return response defined in `response.override` section (see below). This is default behavior: query everything, return fail if failed or return some predefined OK response if everything is good. |
| conditional_routing   | Select single target to query based on conditions (see targets config below), query it and return it's response.                                                                                                                                                                                                              |

Any target may have `condition` parameter which restricts allowance of the target to query it.
This condition is predicate based on request's headers or body content.
If target's condition is false, that target will be excluded from the allowed list and won't be queried.
Exception is `conditional_routing` strategy where all targets must have conditions
but after evaluation of all conditions only single one can be true
and only this one target will be queried for response.
More about conditions configuration is in the targets config section.

#### Listener: `headers`

Format: list of objects.

Default: empty (don't touch headers except Host).

This parameter is a list of transformations
which should be applied to the original request's headers before proceeding to querying and evaluating targets.
Using this config, we can add, change or drop some headers from request.
Each element of the list is a single transformation action, actions will be applied in the list order.

By default, all (untransformed) headers will be passed to targets without changes.

Possible transformations:

- `add` - creates new header with specified name and value, but if header already exists - transformation will be
  ignored.
- `update` - change value of existing header with specified name, if header doesn't exist - transformation will be
  ignored.
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

***Important note***:

> - if you strictly need to set header to some specific value regardless of header's presence you should add two
    consecutive actions - `add` and `update` with the same `value`, order doesn't matter: if you try to update
    non-existent header - new one won't appear, if you try to add existing header - value won't be changed.
> - if you need to guarantee some stable set of headers instead of requested, drop all headers (`drop: "*"`) as first
    action and add all necessary ones as following actions.

#### Listener: `targets`

Format: list of objects.

Default: no defaults, at least one target should be defined.

Target config includes the following parameters:

- `id`: unique (among the listener's targets) target name/ID, default is `TARGET-<url>`
- `url`: full URL of the target
- `tls`: the same as [listener TLS config](#listener-tls), by default listeners' config is used, but if it's defined on
  the target level, it overrides listeners' values.
  Be careful: if you disabled TLS verification of listener but need to use
  custom root CA certificate on target, then you have to enable TLS verification on target.
- `headers`: target's headers transformations, [like request's config](#listener-headers), empty by default
- `body`: create new body if defined, or pass original body by default
- `timeout`: time to wait for response from the target, [like listener's config](#listener-timeout), default is `60s`
- `on_error`: what to do if error occurred during request, default is `propagate`, see explanation below
- `error_status`: what status should be returned from the target if `on_error` set to `status`, usually (but not
  mandatory) this is something like `500`
- `condition`: predicate expression to calculate before request, if value is `false` this target will be excluded from
  the list of allowed targets, default is `true`, see details below

##### Listener: `targets.on_error`

Parameter `on_error` defines target's behavior in case of any error like request timeout, network error, application
problems or any other cases when getting response (even unsuccessful) is impossible.
Possible values:

- `propagate`: return reasonable status code which points to cause of error (if possible), this is some `5xx` status.
- `status`: regardless of error nature, return some specific status code that should be defined in `error_status`
  parameter.
- `drop`: remove this target from the list of responses — it won't be even considered as possible response during
  post-processing of results.

##### Listener: `targets.condition`

This parameter defines some conditional expression (predicate)
which should be true to allow querying of this particular target.
So condition expression has to be evaluated to boolean only.
Expression syntax is the same as [`jq`](https://jqlang.github.io/jq/manual/#basic-filters) utility has.
Root contains the following objects:

- `body`: JSON body on original request (before body processing if `target.body` is defined)
- `env`: target request context - list of name/value pairs of environment variables (before applying target's context)
- `request`: complex object with original request's attributes (before applying of any target's transformation)
    - `headers`: list of name/value pairs with request headers (***headers names are in lower case***)
    - `uri`: complex object
        - `full`: full URI staring
        - `host`: host part of URI
        - `path`: path
        - `query`: query string of URI

Special case of condition expression (and actually default value) is word `default` instead of predicate,
that means this condition is true and target have to be queried anyway.
This condition expression should be used with `conditional_routing` strategy
to mark target which will be queried in case if all other conditions are false.
If strategy is `conditional_routing` there can be only one target with `default` condition.

Some additional explanation:

- if strategy is any except `conditional_routing`:
    - condition is empty (absent) or `default` - target will be queried anyway
    - condition is defined as `jq` expression - target will be queried in case expression evaluating to `true` only
- if strategy is `conditional_routing`:
    - only one condition is true — this target will be queried
    - all conditions are false, and there is one target with `default` condition - this default will be queried
    - all conditions are false, and there is no targets with `default` condition - error will be propagated
    - more than one condition is true — error will be propagated

In other words `conditional_routing` ensure querying of single target only which satisfy its condition.

Condition examples:

- `.env["CTX_REQUEST_HOST"] == "www.google.com"`
- `.request.headers["x-auth-token"] != ""`
- `.body.some.body.int.value == 5`
- `.body.data.products[]|length > 0`
- `default`

##### Listener: `target` config examples

Query www.example.com if request has any non-empty path and forward all requests to logger unconditionally:

```yaml
listeners:
  - targets:
      - url: https://www.example.com/${CTX_REQUEST_PATH}
        id: query
        condition: .env[CTX_REQUEST_PATH] != ""
      - url: http://query-logger:9090/
```

Query www.example.com if request's header `X-Route-To-Query` is `true` and forward all requests to logger
unconditionally:

```yaml
listeners:
  - targets:
      - url: https://www.example.com/${CTX_REQUEST_PATH}
        id: query
        condition: .request.headers["x-route-to-query"] == "true"
      - url: http://query-logger:9090/
```

Conditional routing depending on value in body with logging of unknown queries and transformation of queries:

```yaml
listeners:
  - strategy: conditional_routing
    targets:
      - url: https://www.example.com/path-1
        condition: .body.data.value == 1
        id: query-1
        body: |
          {
            "query": "${CTX_REQUEST_QUERY:-}",
            "path": "${CTX_REQUEST_PATH:-}"
          }
        headers:
          - drop: content-length
          - drop: content-type
          - add: content-type
            value: application/json
      - url: https://www.example.com/path-2
        id: query-2
        condition: .body.data.value == 2
        headers:
          - update: User-Agent
            value: ${CTX_APP_NAME}/${CTX_APP_VERSION}
      - url: https://www.example.com/path-3
        id: query-3
        condition: .body.data.value == 3
        on_error: status
        error_status: 200
      - url: http://bad-queries-logger:3333/
        headers:
          - drop: Authorization
        condition: default
```

***Important notes***:

> - if you change request body remember to drop `content-length` header and add/update `content-type` header, otherwise
    request handler will panic due to request inconsistency.

#### Listener: `response`

Format: object definition.

Default:

```yaml
response:
  failed_status_regex: "4\d{2}|5\d{2}"
  no_targets_status: 500
```

This parameter defines how to transform or override (create) response before returning it to the requester.
Allowed parameters are:

- `target_selector`: target ID to select for response in case of `*_target_id` strategy is configured, this parameter is
  mandatory for such strategies and allowed in this case only.
- `failed_status_regex`: regex to assess if response status should be interpreted as failed, reasonable default includes
  all `4xx` and `5xx` statuses.
- `no_targets_status`: which status code should be returned in case when no targets to query (all conditions are false)
  or all responses were dropped due to `on_error: drop` target's parameter and strategy is `*_target_id` or `*_ok`.
- `override`: response override config (see below), optional

Response override config intended to provide custom (overridden) response parts such as body, headers, and status code.
So you can define three parameters here:

- `body`: overrides body content in response
- `headers`: defines header transformations similar to [this](#listener-headers)
- `status`: set particular response status instead of original value

***Important notes:***

> - the default behavior of those overrides is to pass original content of body and headers and status.
    But for all `*_override` strategies defaults is empty body,
    empty headers and status 200. So if you need to create exactly
    new response instead of the one obtained from some target, you can (or have to) define those overrides.
    If you omit this config than `*_override` strategy returns empty response with status 200.
> - if you change response body remember to drop `content-length` header and add/update `content-type` header, otherwise
    request handler will panic due to response inconsistency.

### Huge configuration example

Below is an example of almost all possible configuration parameters with some explanations.

```yaml
listeners:
  - id: Listener-8080 # default is LISTENER-<on value>
    listen_on: "*:8080" # or ip:port like 1.2.3.4:1234, or just port number
    timeout: 10s
    methods: # default is an empty list that means "any method"
      - GET
      - POST
    strategy: failed_then_override # default

    headers:
      - drop: "*" # special case, default is preserve everything except Host
      - add: X-Added-Header
        value: something
      - update: Authorization
        value: ${HTTP_ENV_SOME_AUTH_TOKEN}
      - drop: X-Forwarded-For
      - update: User-Agent
        value: "this was ${CTX_REQUEST_HEADERS_USER_AGENT}"

    targets:
      - id: Target-0 # default is TARGET-<url value>
        url: https://qqq.www.com/
        timeout: 60s
        body: '{"method": "${CTX_REQUEST_METHOD}"}'
        headers:
          - drop: content-length
          - drop: content-type
          - add: content-type
            value: application/json
        on_error: status
        error_status: 555
      - id: Target-1
        condition: .body.target == "1"
        url: https://qqq.www.com/${CTX_REQUEST_PATH}
      - id: Target-2
        url: https://qqq.www.com/${CTX_REQUEST_PATH}?${CTX_REQUEST_QUERY}
        headers:
          - update: Authorization
            value: ${HTTP_ENV_TARGET_2_AUTH_TOKEN}
          - drop: X-Added-Header

    response:
      failed_status_regex: "3\\d{2}|4\\d{2}|5\\d{2}"
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
          - drop: content-length
          - drop: content-type
          - add: content-type
            value: application/json
          - add: X-Http-Dragonfly-Version
            value: ${CTX_APP_VERSION}
          - add: X-Http-Dragonfly-Response-Source
            value: ${CTX_TARGET_ID:-unknown}

  - id: Condition-plus-target_id-8081
    listen_on: "*:8081"
    timeout: 5s
    strategy: always_target_id
    methods:
      - PUT
      - POST
    targets:
      - id: Target-0
        condition: .body.target == "0"
        url: https://qqq.www.com/
        timeout: 60s
      - id: Target-1
        condition: .headers["qqq"] == "WWW"
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
    strategy: conditional_routing
    targets:
      - id: Target-0
        condition: .body.target == "0" and .env["CTX_REQUEST_METHOD"] != "POST"
        url: https://qqq.www.com/
        timeout: 60s
      - id: Target-1
        condition: .headers["qqq"] == "WWW" and .env["CTX_REQUEST_METHOD"] != "POST"
        url: https://qqq.www.com/${CTX_REQUEST_PATH}
      - id: Target-2
        condition: .env["CTX_REQUEST_METHOD"] == "POST"
        url: https://qqq.www.com/${CTX_REQUEST_PATH}?${CTX_REQUEST_QUERY}
    response:
      no_targets_status: 200

      override:
        headers:
          - add: X-Http-Conditional-Response-Source
            value: ${CTX_TARGET_ID}
```
