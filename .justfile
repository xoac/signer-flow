
# Guard for running just without args. just list recipes
recipes-list:
    just --list

signer_rest_api_image := "sylwekrapala/signer-rest-api"
signer_service_image := "sylwekrapala/signer-service"

docker-build tag="edge":
  docker build -f ./signer-rest-api.dockerfile -t {{ signer_rest_api_image }}:{{ tag }} .
  docker build -f ./signer-service.dockerfile -t {{ signer_service_image }}:{{ tag }} .

docker-build-push tag="edge": (docker-build tag )
  docker push {{ signer_rest_api_image }}:{{ tag }} 
  docker push {{ signer_service_image }}:{{ tag }}
  

# build application from Dockerfile and push it
minikube-update-app tag="edge": (docker-build-push  tag )
  -kubectl delete -f ./k8s/singer-flow.yaml
  kubectl apply -f ./k8s/singer-flow.yaml

