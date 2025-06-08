EC2_HOST=ubuntu@$(terraform -chdir=./terraform output -raw typesense_private_ip)

ssh -A $EC2_HOST
