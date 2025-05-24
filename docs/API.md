# API

There is an OpenAPI spec [Here](./api.yml) you can use an online viewer for this such as [Swagger Editor](https://editor.swagger.io/) or a IDE extension.

You can run a local docs website using:

```sh
npx @redocly/cli preview-docs ./docs/api.yml
```

## API Parts

The API is divided into two portions:

- Admin (/admin/)
- Box (/box/)

The admin portion is routes that are only for developer and restricted use (setting up & deleting tenants, searching across document boxes)

The Box portion is what user requests should proxy towards

To validate the users access to resources you can check the URL structure for the scope they are attempting to access (READ)

All access to a document box of a specific scope will be done through:

```sh
POST/GET/DELETE/PUT /box/{scope}/
```

The exception being the endpoint for creating a document box:

```sh
POST /box/
```

This endpoint takes a "scope" field as the JSON body which should be used when
validating the users access to create a document box

```json
{
  "scope": "string"
}
```

All endpoints should proxy the user content directly as is, there is one endpoint that will require special handling (File upload):

```sh
POST /box/{scope}/file
```

This is a `multipart/form-data` request, ensure all the parts are passed correctly. The `content-type` header set on the file part of the data is used as the file mime type so ensure this is passed along correctly

## Tenant & User headers

For all `/box/` endpoints a `x-tenant-id` header is required. This should be the UUID of the specific tenant the request is operating within.

When the action is being executed on behalf of a user, specify the following headers:

* `x-user-id` - The ID of the user making the action (Omit if not acting on behalf of a user, don't provide null)
* `x-user-name` - The name of the user making the action (Omit if not acting on behalf of a user or username is not present, don't provide null)
* `x-user-image-id` - The image ID of the user making the action (Omit if not acting on behalf of a user or image ID is not present, don't provide null)