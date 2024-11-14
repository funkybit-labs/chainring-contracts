
build:
	docker build --platform=linux/amd64 -t ord .

package_container:
	docker build --platform=linux/amd64 -t 851725450525.dkr.ecr.us-east-2.amazonaws.com/ord .

push_container:
	AWS_SDK_LOAD_CONFIG=true docker push 851725450525.dkr.ecr.us-east-2.amazonaws.com/ord:latest
