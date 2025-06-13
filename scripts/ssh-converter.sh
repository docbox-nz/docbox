EC2_HOST=admin@$(terraform -chdir=./terraform output -raw converter_private_ip)

ssh -A $EC2_HOST
