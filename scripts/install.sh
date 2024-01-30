#!/usr/bin/env bash
set -eu

repo="trdthg/t-autotest"

echo ">>> checking latest tag..."
tag=$(wget -q -O- https://api.github.com/repos/${repo}/releases/latest | jq -r '.tag_name')
echo "<<< latest tag: $tag"

echo ">>> prepare dir"
folder="$HOME/.autotest"
mkdir -p $folder
echo "<<< prepare success"

echo ">>> downloading zip..."
cd $folder
zip_name="autotest-linux.tar.gz"
wget https://github.com/trdthg/t-autotest/releases/download/$tag/$zip_name -O $zip_name
echo "<<< download success"

echo ">>> extracting..."
tar -zxvf $zip_name
echo "<<< extrace success"

cd --

echo ">>> setting env(.bashrc)..."
echo "export PATH=$PATH:$folder" >> ~/.bashrc
export PATH="$PATH:$folder"
echo "<<< done!, you can try with 'autotest -v'"
